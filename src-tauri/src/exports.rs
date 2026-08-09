// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

mod media_preview;
mod track_selection;
mod window;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};
use serde::{Deserialize, Serialize};
use tauri::{image::Image, ipc::Response, AppHandle, Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::recording::FinalizeInfo;
use crate::screenshots::{encode_png, screenshot_directory, unique_path, CapturedImage};

const EXPORT_CHANGED_EVENT: &str = "export://artifact";
const EXPORT_PROGRESS_EVENT: &str = "export://progress";
const EXPORT_DIRECTORY_FILE: &str = "export-directory.json";
const SCREENSHOT_EXTENSION: &str = "png";
/// What a saved recording is delivered as when it can be, which is whenever
/// FFmpeg is on the machine. See [`save_recording`] for the other case.
const RECORDING_EXTENSION: &str = "mp4";
/// The container a recording is written to while it runs. It is a QuickTime
/// movie because that is the only container that survives being fragmented,
/// and only a fragmented file is worth anything if the app dies mid-recording
/// - see `recording::platform::Container::quicktime_fragmented`.
const WORKING_RECORDING_EXTENSION: &str = "mov";
/// Every extension a working recording can be found under in the recordings
/// directory. `.mp4` is there for the files an earlier version of the app left
/// behind: an upgrade must not walk past someone's unsaved recording.
const WORKING_RECORDING_EXTENSIONS: &[&str] = &[WORKING_RECORDING_EXTENSION, RECORDING_EXTENSION];
/// How long an unclaimed recording is kept before it is swept away. Long
/// enough that a crash is recoverable, short enough that a forgotten one does
/// not sit in the app's data directory forever.
const ORPHAN_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// The long edge of the preview shipped to the window. The capture itself can
/// be 30 MB of pixels, which has no business crossing the IPC boundary.
const PREVIEW_MAX_EDGE: u32 = 640;
const MAX_FILE_STEM: usize = 200;

/// A capture waiting to be saved.
///
/// The window renders itself by artifact kind rather than assuming a
/// screenshot, because a recording is a file on disk rather than pixels in
/// memory and almost nothing about handling it is the same.
pub enum ExportArtifact {
  Screenshot {
    /// Unique per capture. Two consecutive fullscreen captures are identical
    /// in every other respect, so the window needs this to tell them apart
    /// and start the new one at fit rather than inheriting the old zoom.
    id: u64,
    image: CapturedImage,
    suggested_file_stem: String,
  },
  Recording {
    audio_tracks: Vec<RecordingAudioTrack>,
    id: u64,
    duration_ms: u64,
    height: u32,
    /// The working file. Saving moves it or derives the requested compressed
    /// copy; discarding deletes it.
    path: PathBuf,
    source_scale_percent: u16,
    suggested_file_stem: String,
    width: u32,
  },
}

/// What the window is told about the pending artifact. Deliberately without
/// pixels: the preview travels separately, as bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(
  rename_all = "camelCase",
  rename_all_fields = "camelCase",
  tag = "kind"
)]
pub enum ExportArtifactSnapshot {
  Screenshot {
    id: u64,
    suggested_file_stem: String,
    extension: String,
    width: u32,
    height: u32,
  },
  Recording {
    audio_tracks: Vec<RecordingAudioTrack>,
    can_compress: bool,
    id: u64,
    suggested_file_stem: String,
    extension: String,
    width: u32,
    height: u32,
    duration_ms: u64,
    original_size_bytes: u64,
    /// The working file, for the window to play through the asset protocol.
    /// Scoped to the recordings directory in `tauri.conf.json`, which is the
    /// only place this path can ever point.
    path: PathBuf,
    source_scale_percent: u16,
  },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AudioTrackKind {
  SystemAudio,
  Microphone,
  Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingAudioTrack {
  pub kind: AudioTrackKind,
  pub label: String,
  pub stream_index: usize,
}

fn recording_audio_tracks(
  has_system_audio: bool,
  has_microphone: bool,
) -> Vec<RecordingAudioTrack> {
  let mut tracks = Vec::with_capacity(usize::from(has_system_audio) + usize::from(has_microphone));
  if has_system_audio {
    tracks.push(RecordingAudioTrack {
      kind: AudioTrackKind::SystemAudio,
      label: "System audio".to_owned(),
      stream_index: tracks.len(),
    });
  }
  if has_microphone {
    tracks.push(RecordingAudioTrack {
      kind: AudioTrackKind::Microphone,
      label: "Microphone".to_owned(),
      stream_index: tracks.len(),
    });
  }
  tracks
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSnapshot {
  pub artifact: Option<ExportArtifactSnapshot>,
  pub directory: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportProgress {
  artifact_id: u64,
  processed_ms: u64,
}

#[derive(Clone)]
struct ActiveExportJob {
  artifact_id: u64,
  cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct ExportState {
  active_export: Mutex<Option<ActiveExportJob>>,
  artifact: Mutex<Option<ExportArtifact>>,
  generation: AtomicU64,
  preview: Mutex<Option<Vec<u8>>>,
  /// Built only if the user zooms in, because it is the whole capture.
  full_preview: Mutex<Option<Vec<u8>>>,
  directory: Mutex<Option<PathBuf>>,
  recording_preview: Mutex<Option<media_preview::RecordingPreview>>,
  recording_preview_preparation: Mutex<()>,
  preview_mixes: Mutex<media_preview::PreviewMixes>,
  preview_mix_preparation: Mutex<()>,
  compression_estimates: Mutex<HashMap<(u64, u8, u16), u64>>,
  compression_estimate_preparation: Mutex<()>,
}

/// Characters Windows forbids outright. macOS only objects to `/` and `:`, so
/// stripping the Windows set keeps a name portable between the two.
const ILLEGAL_CHARACTERS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Names Windows reserves whatever the extension is.
const RESERVED_STEMS: &[&str] = &[
  "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
  "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Cleans a user-typed file name into something both platforms will accept, or
/// `None` if nothing usable is left.
///
/// Illegal characters are stripped rather than rejected: a name is a label, and
/// silently dropping a colon is friendlier than refusing to save over one.
pub fn sanitize_file_stem(input: &str) -> Option<String> {
  let stripped: String = input
    .chars()
    .filter(|character| !ILLEGAL_CHARACTERS.contains(character) && !character.is_control())
    .collect();
  // Windows silently drops trailing dots and spaces, which would leave the
  // saved file under a different name than the one shown.
  let trimmed = stripped.trim().trim_end_matches(['.', ' ']).trim();

  if trimmed.is_empty() {
    return None;
  }
  if RESERVED_STEMS
    .iter()
    .any(|reserved| trimmed.eq_ignore_ascii_case(reserved))
  {
    return None;
  }

  let mut stem = trimmed.to_owned();
  if stem.len() > MAX_FILE_STEM {
    stem = stem.chars().take(MAX_FILE_STEM).collect::<String>();
    stem = stem.trim().to_owned();
  }

  (!stem.is_empty()).then_some(stem)
}

fn directory_path(app: &AppHandle) -> tauri::Result<PathBuf> {
  Ok(app.path().app_config_dir()?.join(EXPORT_DIRECTORY_FILE))
}

fn load_directory(app: &AppHandle) -> Option<PathBuf> {
  let stored = directory_path(app)
    .ok()
    .and_then(|path| std::fs::read(path).ok())
    .and_then(|contents| serde_json::from_slice::<PathBuf>(&contents).ok())?;

  stored.is_dir().then_some(stored)
}

fn store_directory(app: &AppHandle, directory: &Path) -> tauri::Result<()> {
  let path = directory_path(app)?;
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)?;
  }
  let contents = serde_json::to_vec_pretty(directory).map_err(std::io::Error::other)?;
  std::fs::write(path, contents)?;

  Ok(())
}

/// The folder the next export lands in: whatever was used last, falling back to
/// the platform's own screenshot folder on a first run.
fn current_directory(app: &AppHandle) -> Option<PathBuf> {
  let state = app.state::<ExportState>();
  let remembered = state
    .directory
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .clone();

  remembered.or_else(|| screenshot_directory(app).ok())
}

fn snapshot(app: &AppHandle) -> ExportSnapshot {
  let state = app.state::<ExportState>();
  let artifact = state
    .artifact
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .as_ref()
    .map(|artifact| match artifact {
      ExportArtifact::Screenshot {
        id,
        image,
        suggested_file_stem,
      } => ExportArtifactSnapshot::Screenshot {
        id: *id,
        suggested_file_stem: suggested_file_stem.clone(),
        extension: SCREENSHOT_EXTENSION.to_owned(),
        width: image.width,
        height: image.height,
      },
      ExportArtifact::Recording {
        audio_tracks,
        duration_ms,
        height,
        id,
        path,
        source_scale_percent,
        suggested_file_stem,
        width,
      } => ExportArtifactSnapshot::Recording {
        audio_tracks: audio_tracks.clone(),
        can_compress: media_preview::supports_compression(),
        id: *id,
        suggested_file_stem: suggested_file_stem.clone(),
        extension: delivered_extension(path, media_preview::remuxer().is_some()).to_owned(),
        width: *width,
        height: *height,
        duration_ms: *duration_ms,
        original_size_bytes: std::fs::metadata(path).map_or(0, |metadata| metadata.len()),
        path: path.clone(),
        source_scale_percent: *source_scale_percent,
      },
    });

  ExportSnapshot {
    artifact,
    directory: current_directory(app),
  }
}

fn emit_snapshot(app: &AppHandle) {
  let _ = app.emit(EXPORT_CHANGED_EVENT, snapshot(app));
}

/// Shrinks the capture to something worth sending over IPC.
fn preview_png(image: &CapturedImage) -> Option<Vec<u8>> {
  let buffer = image::RgbaImage::from_raw(image.width, image.height, image.rgba.clone())?;
  let scale = f64::from(PREVIEW_MAX_EDGE) / f64::from(image.width.max(image.height));
  let (width, height) = if scale >= 1.0 {
    (image.width, image.height)
  } else {
    (
      ((f64::from(image.width) * scale).round() as u32).max(1),
      ((f64::from(image.height) * scale).round() as u32).max(1),
    )
  };

  let thumbnail = image::DynamicImage::ImageRgba8(buffer).thumbnail(width, height);
  let mut png = Vec::new();
  thumbnail
    .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
    .ok()?;

  Some(png)
}

/// The capture at full resolution, for zooming into. Encoded losslessly and
/// quickly - this is for looking at, not for keeping, so the slow quantizing
/// encoder that produces the saved file would be the wrong trade here.
fn full_preview_png(image: &CapturedImage) -> Result<Vec<u8>, String> {
  let mut png = Vec::new();
  PngEncoder::new_with_quality(
    std::io::Cursor::new(&mut png),
    CompressionType::Fast,
    FilterType::Sub,
  )
  .write_image(
    &image.rgba,
    image.width,
    image.height,
    ExtendedColorType::Rgba8,
  )
  .map_err(|error| error.to_string())?;

  Ok(png)
}

/// Deletes the working file behind a recording that will not be saved.
///
/// A screenshot lives in memory and needs nothing; a recording is a file, and
/// every path that lets go of one without saving it comes through here.
fn delete_working_file(artifact: &ExportArtifact) {
  if let ExportArtifact::Recording { path, .. } = artifact {
    let _ = std::fs::remove_file(path);
  }
}

/// Removes everything built for the artifact that is going away.
///
/// Every path that lets go of a recording - discarding it, replacing it with a
/// new capture, saving it - comes through here, so no derivative outlives the
/// artifact it was made from.
fn clear_recording_preview(app: &AppHandle) {
  let state = app.state::<ExportState>();
  state
    .recording_preview
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .take();
  state
    .preview_mixes
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .cleanup();
  state
    .compression_estimates
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .clear();
}

/// The next artifact identity. Two consecutive captures are otherwise
/// indistinguishable, and the window needs to tell them apart.
fn next_id(app: &AppHandle) -> u64 {
  app
    .state::<ExportState>()
    .generation
    .fetch_add(1, Ordering::SeqCst)
    .wrapping_add(1)
}

/// Puts an artifact in front of the user, replacing whatever was waiting.
fn present(
  app: &AppHandle,
  artifact: ExportArtifact,
  preview: Option<Vec<u8>>,
) -> Result<(), String> {
  clear_recording_preview(app);
  {
    let state = app.state::<ExportState>();
    *state
      .preview
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner()) = preview;
    *state
      .full_preview
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    let replaced = state
      .artifact
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .replace(artifact);
    // Taking a screenshot while a recording waits to be saved abandons the
    // recording, and an abandoned recording must not outlive its window.
    if let Some(replaced) = replaced {
      delete_working_file(&replaced);
    }
  }

  window::show(app).map_err(|error| error.to_string())?;
  emit_snapshot(app);

  Ok(())
}

/// Hands a freshly captured still to the export window.
pub fn present_screenshot(
  app: &AppHandle,
  image: CapturedImage,
  suggested_file_stem: String,
) -> Result<(), String> {
  let preview = preview_png(&image);

  present(
    app,
    ExportArtifact::Screenshot {
      id: next_id(app),
      image,
      suggested_file_stem,
    },
    preview,
  )
}

/// Hands a finished recording to the export window.
///
/// Mirrors `present_screenshot`, with the poster standing in for the preview.
/// There is no full-resolution counterpart: the artifact is a movie, and the
/// only still it has is the one drawn when it finished.
pub fn present_recording(
  app: &AppHandle,
  info: FinalizeInfo,
  suggested_file_stem: String,
) -> Result<(), String> {
  let FinalizeInfo {
    has_microphone,
    has_system_audio,
    duration_ms,
    height,
    path,
    poster,
    source_scale_factor,
    width,
  } = info;

  let mut audio_tracks = recording_audio_tracks(has_system_audio, has_microphone);
  if audio_tracks.is_empty() {
    audio_tracks = media_preview::inspect_audio_tracks(&path).unwrap_or_default();
  }

  present(
    app,
    ExportArtifact::Recording {
      id: next_id(app),
      audio_tracks,
      duration_ms,
      height,
      path,
      source_scale_percent: scale_percent(source_scale_factor),
      suggested_file_stem,
      width,
    },
    poster,
  )
}

fn take_artifact(app: &AppHandle) -> Option<ExportArtifact> {
  clear_recording_preview(app);
  let state = app.state::<ExportState>();
  let _ = state
    .preview
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .take();
  let _ = state
    .full_preview
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .take();

  let artifact = state
    .artifact
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .take();

  artifact
}

/// Drops the pending artifact and puts the window away. Cancelling and closing
/// the window are the same act.
pub fn discard(app: &AppHandle) {
  if let Some(artifact) = take_artifact(app) {
    delete_working_file(&artifact);
  }
  let _ = window::hide(app);
  emit_snapshot(app);
}

#[tauri::command]
pub fn get_export_snapshot(app: AppHandle) -> ExportSnapshot {
  snapshot(&app)
}

/// The thumbnail by default; the full-resolution capture only once something
/// actually needs it, and cached from then on.
#[tauri::command]
pub async fn get_export_preview(app: AppHandle, full: bool) -> Result<Response, String> {
  let bytes = tauri::async_runtime::spawn_blocking(move || {
    let state = app.state::<ExportState>();
    let missing = || "There is nothing waiting to be exported".to_owned();

    if !full {
      return state
        .preview
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .ok_or_else(missing);
    }

    if let Some(cached) = state
      .full_preview
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .clone()
    {
      return Ok(cached);
    }

    let encoded = {
      let artifact = state
        .artifact
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
      // A recording has no full-resolution still to zoom into, so the poster
      // it was presented with is all there is.
      let Some(ExportArtifact::Screenshot { image, .. }) = artifact.as_ref() else {
        return Err(missing());
      };
      full_preview_png(image)?
    };
    *state
      .full_preview
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(encoded.clone());

    Ok(encoded)
  })
  .await
  .map_err(|error| error.to_string())??;

  Ok(Response::new(bytes))
}

/// Prepares independently playable audio tracks and compact waveform data for
/// the recording currently shown in the export window.
///
/// The preparation mutex is intentional: React may mount an effect twice in a
/// development build, and two FFmpeg processes must never race to replace the
/// same preview files. The second caller waits, then takes the cached result.
#[tauri::command]
pub async fn get_recording_preview(
  app: AppHandle,
  artifact_id: u64,
) -> Result<media_preview::RecordingPreview, String> {
  tauri::async_runtime::spawn_blocking(move || {
    let state = app.state::<ExportState>();
    let _preparing = state
      .recording_preview_preparation
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());

    if let Some(preview) = state
      .recording_preview
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .as_ref()
      .filter(|preview| preview.artifact_id == artifact_id)
      .cloned()
    {
      return Ok(preview);
    }

    let (path, duration_ms, tracks) = {
      let artifact = state
        .artifact
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
      let Some(ExportArtifact::Recording {
        audio_tracks,
        duration_ms,
        id,
        path,
        ..
      }) = artifact.as_ref()
      else {
        return Err("There is no recording to preview".to_owned());
      };
      if *id != artifact_id {
        return Err("That recording is no longer waiting to be exported".to_owned());
      }
      (path.clone(), *duration_ms, audio_tracks.clone())
    };

    let preview = media_preview::prepare(artifact_id, &path, duration_ms, &tracks)?;
    let is_current = state
      .artifact
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .as_ref()
      .is_some_and(
        |artifact| matches!(artifact, ExportArtifact::Recording { id, .. } if *id == artifact_id),
      );
    if !is_current {
      return Err("That recording is no longer waiting to be exported".to_owned());
    }

    state
      .recording_preview
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .replace(preview.clone());
    Ok(preview)
  })
  .await
  .map_err(|error| error.to_string())?
}

/// The single file the export window should play for a set of enabled tracks.
///
/// The window plays one `<video>` and nothing else. Playing the recording
/// alongside separate `<audio>` elements and correcting their drift was tried
/// first and cannot be made to work: setting `currentTime` on a media element
/// is a seek, a seek silences it until its decoder catches up, and the video
/// never stops advancing meanwhile - so each correction arrives before the
/// last one produced any sound. Handing the element one already-muxed file
/// makes the whole class of problem disappear, because there is only one clock.
///
/// What comes back is a *playback* file. See [`track_selection`] for why that
/// is not what saving the recording produces.
#[tauri::command]
pub async fn get_recording_preview_mix(
  app: AppHandle,
  artifact_id: u64,
  enabled_stream_indices: Vec<usize>,
) -> Result<PathBuf, String> {
  tauri::async_runtime::spawn_blocking(move || {
    let state = app.state::<ExportState>();
    // Held for the same reason the track preparation is: a debounced toggle
    // and a mount effect can ask at once, and two FFmpeg processes must never
    // race to write the same file. The second caller waits and finds it built.
    let _preparing = state
      .preview_mix_preparation
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());

    let (path, tracks) = {
      let artifact = state
        .artifact
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
      let Some(ExportArtifact::Recording {
        audio_tracks,
        id,
        path,
        ..
      }) = artifact.as_ref()
      else {
        return Err("There is no recording to preview".to_owned());
      };
      if *id != artifact_id {
        return Err("That recording is no longer waiting to be exported".to_owned());
      }
      (path.clone(), audio_tracks.clone())
    };

    let selection = track_selection::TrackSelection::new(&tracks, &enabled_stream_indices);
    let mixed = {
      let mut mixes = state
        .preview_mixes
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
      media_preview::preview_mix(&mut mixes, artifact_id, &path, &tracks, &selection)?
    };

    // The recording can be discarded while FFmpeg is still working, and the
    // cleanup that ran then knew nothing about a file that did not yet exist.
    let is_current = state
      .artifact
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .as_ref()
      .is_some_and(
        |artifact| matches!(artifact, ExportArtifact::Recording { id, .. } if *id == artifact_id),
      );
    if !is_current {
      state
        .preview_mixes
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .cleanup();
      return Err("That recording is no longer waiting to be exported".to_owned());
    }

    Ok(mixed)
  })
  .await
  .map_err(|error| error.to_string())?
}

/// A sampled estimate of the file the current export choices would produce.
/// Video is the expensive unknown; selected AAC sizes are derived from their
/// actual configured bitrates and added after the sample is extrapolated.
#[tauri::command]
pub async fn estimate_recording_export(
  app: AppHandle,
  artifact_id: u64,
  enabled_stream_indices: Vec<usize>,
  collapse_audio: bool,
  compression: u8,
  resolution_scale_percent: u16,
) -> Result<u64, String> {
  if compression > 4 {
    return Err("Compression must be between 0 and 4".to_owned());
  }

  tauri::async_runtime::spawn_blocking(move || {
    let state = app.state::<ExportState>();
    let (path, tracks, duration_ms, original_size, has_video, source_scale_percent) = {
      let artifact = state
        .artifact
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
      let Some(ExportArtifact::Recording {
        audio_tracks,
        duration_ms,
        height,
        id,
        path,
        source_scale_percent,
        width,
        ..
      }) = artifact.as_ref()
      else {
        return Err("There is no recording to estimate".to_owned());
      };
      if *id != artifact_id {
        return Err("That recording is no longer waiting to be exported".to_owned());
      }
      (
        path.clone(),
        audio_tracks.clone(),
        *duration_ms,
        std::fs::metadata(path).map_or(0, |metadata| metadata.len()),
        *width > 0 && *height > 0,
        *source_scale_percent,
      )
    };
    validate_resolution_scale(resolution_scale_percent, source_scale_percent)?;

    let selection = track_selection::TrackSelection::new(&tracks, &enabled_stream_indices);
    let layout = if collapse_audio {
      track_selection::AudioLayout::Mixdown
    } else {
      track_selection::AudioLayout::SeparateTracks
    };
    let selected_audio = selection.estimated_audio_bytes(&tracks, layout, duration_ms);

    // The original route only remuxes, so the source size minus its known AAC
    // streams is the best video measurement available without parsing it.
    if compression == 0 && resolution_scale_percent == source_scale_percent {
      let all_indices = tracks
        .iter()
        .map(|track| track.stream_index)
        .collect::<Vec<_>>();
      let all = track_selection::TrackSelection::new(&tracks, &all_indices);
      let original_audio = all.estimated_audio_bytes(
        &tracks,
        track_selection::AudioLayout::SeparateTracks,
        duration_ms,
      );
      return Ok(
        original_size
          .saturating_sub(original_audio)
          .saturating_add(selected_audio),
      );
    }

    let _preparing = state
      .compression_estimate_preparation
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    let key = (artifact_id, compression, resolution_scale_percent);
    let cached = state
      .compression_estimates
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .get(&key)
      .copied();
    let video = match cached {
      Some(bytes) => bytes,
      None if !has_video => 0,
      None => {
        let bytes = media_preview::estimate_compressed_video_bytes(
          &path,
          duration_ms,
          compression,
          source_scale_percent,
          resolution_scale_percent,
        )?;
        state
          .compression_estimates
          .lock()
          .unwrap_or_else(|poisoned| poisoned.into_inner())
          .insert(key, bytes);
        bytes
      }
    };

    // MP4's tables are small but real. Half a percent plus its fixed headers
    // keeps the estimate honest without pretending CRF can predict exact size.
    let media = video.saturating_add(selected_audio);
    Ok(media.saturating_add(media / 200).saturating_add(4_096))
  })
  .await
  .map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn cancel_export(app: AppHandle) {
  discard(&app);
}

/// Requests cancellation of the save currently processing, if there is one.
///
/// The worker owns the FFmpeg child and performs the actual kill and wait. The
/// command only flips its token, so it never blocks the window thread or races
/// another thread for mutable access to the process.
#[tauri::command]
pub fn cancel_export_job(app: AppHandle) -> bool {
  let state = app.state::<ExportState>();
  let active = state
    .active_export
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());
  let Some(job) = active.as_ref() else {
    return false;
  };

  job.cancelled.store(true, Ordering::Release);
  true
}

#[tauri::command]
pub fn copy_export_to_clipboard(app: AppHandle) -> Result<(), String> {
  // Refused before the artifact is taken, not after: the clipboard cannot hold
  // a movie, and taking one only to put it back would drop its poster on the
  // way through. The window hides the button, so this is for callers that are
  // out of date rather than for anything a user can press.
  if matches!(
    app
      .state::<ExportState>()
      .artifact
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .as_ref(),
    Some(ExportArtifact::Recording { .. })
  ) {
    return Err("A recording cannot be copied to the clipboard".to_owned());
  }

  let artifact = take_artifact(&app).ok_or_else(|| "There is nothing to copy".to_owned())?;
  let ExportArtifact::Screenshot { image, .. } = artifact else {
    return Err("There is nothing to copy".to_owned());
  };

  app
    .clipboard()
    .write_image(&Image::new(&image.rgba, image.width, image.height))
    .map_err(|error| error.to_string())?;

  let _ = window::hide(&app);
  emit_snapshot(&app);

  Ok(())
}

#[tauri::command]
pub async fn browse_export_directory(app: AppHandle) -> Result<Option<PathBuf>, String> {
  let start = current_directory(&app);
  // Parented to the export window on purpose: left to itself the picker
  // attaches as a sheet to whichever window happens to be first, which for an
  // accessory app is usually one of the hidden overlay panels - and a sheet on
  // a hidden window is an invisible dialog.
  let parent = app.get_webview_window(crate::windows::WindowLabel::Export.as_str());
  let picked = tauri::async_runtime::spawn_blocking(move || {
    use tauri_plugin_dialog::DialogExt;

    let mut dialog = app.dialog().file().set_title("Choose a folder");
    if let Some(start) = start {
      dialog = dialog.set_directory(start);
    }
    if let Some(parent) = &parent {
      dialog = dialog.set_parent(parent);
    }
    dialog.blocking_pick_folder()
  })
  .await
  .map_err(|error| error.to_string())?;

  Ok(picked.and_then(|path| path.into_path().ok()))
}

#[tauri::command]
pub fn set_export_directory(app: AppHandle, directory: PathBuf) -> Result<(), String> {
  if !directory.is_dir() {
    return Err("That folder is no longer available".to_owned());
  }

  *app
    .state::<ExportState>()
    .directory
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(directory.clone());
  store_directory(&app, &directory).map_err(|error| error.to_string())?;
  emit_snapshot(&app);

  Ok(())
}

/// Moves a finished recording to where the user asked for it.
///
/// A rename when it can be, a copy when the destination is on another volume,
/// and never a re-encode: the movie was encoded once, while it was being
/// recorded, and encoding it again would cost minutes and quality for nothing.
fn move_file(from: &Path, to: &Path) -> Result<(), String> {
  match std::fs::rename(from, to) {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::CrossesDevices => {
      std::fs::copy(from, to).map_err(|error| error.to_string())?;
      // The copy is the save; failing to tidy up afterwards is not worth
      // telling the user their recording was not saved.
      let _ = std::fs::remove_file(from);
      Ok(())
    }
    Err(error) => Err(error.to_string()),
  }
}

/// The extension the working recording will actually be saved under.
///
/// `.mp4` whenever it can be remuxed into one, because that is the file
/// everything opens and nobody should have to know what the app records to.
/// Without FFmpeg the QuickTime movie is handed over as it is, under its own
/// name: renaming a `.mov` to `.mp4` would produce a file that lies about
/// itself, and some players trust the name over the bytes.
///
/// Falls back to the working file's own extension rather than a constant, so
/// a recording recovered from an older version - which really is an `.mp4` -
/// is described correctly too.
fn delivered_extension(working: &Path, can_remux: bool) -> &str {
  if can_remux {
    return RECORDING_EXTENSION;
  }

  working
    .extension()
    .and_then(|extension| extension.to_str())
    .unwrap_or(WORKING_RECORDING_EXTENSION)
}

/// Puts a finished recording where the user asked for it, as an .mp4 if that
/// is possible and honestly as what it is if it is not.
///
/// The remux is attempted first and its failure is not an error: FFmpeg being
/// absent, or refusing the file, is no reason to lose a recording the user
/// just asked to keep. Either way exactly one file arrives, and the path
/// returned is the one that was written - never a name the caller assumed.
fn save_recording(
  working: &Path,
  directory: &Path,
  stem: &str,
  remux: Option<media_preview::Remux>,
) -> Result<PathBuf, String> {
  let taken = |candidate: &Path| candidate.exists();
  if let Some(remux) = remux {
    let path = unique_path(directory, stem, RECORDING_EXTENSION, &taken);
    if remux(working, &path).is_ok() {
      // The stream copy is a copy: the working movie is only let go of once
      // its replacement is on disk under its final name.
      let _ = std::fs::remove_file(working);
      return Ok(path);
    }
  }

  let path = unique_path(directory, stem, delivered_extension(working, false), &taken);
  move_file(working, &path)?;

  Ok(path)
}

/// Saves a recording whose audio streams or layout differ from the source.
/// There is deliberately no `.mov` fallback here: keeping the source would
/// also keep tracks the user turned off, or fail to produce the requested
/// mixdown. The working recording remains untouched on every failure.
fn save_selected_recording(
  working: &Path,
  directory: &Path,
  stem: &str,
  selection: &track_selection::TrackSelection,
  layout: track_selection::AudioLayout,
  run: media_preview::ExportRunOptions<'_>,
  exporter: Option<media_preview::SelectedRecordingExport>,
) -> Result<Option<PathBuf>, String> {
  let exporter = exporter.ok_or_else(|| {
    "FFmpeg is required to compress or change which audio tracks are exported".to_owned()
  })?;
  let path = unique_path(directory, stem, RECORDING_EXTENSION, &|candidate| {
    candidate.exists()
  });
  match exporter(working, &path, selection, layout, run)? {
    media_preview::ExportRunResult::Completed => {
      let _ = std::fs::remove_file(working);
      Ok(Some(path))
    }
    media_preview::ExportRunResult::Cancelled => Ok(None),
  }
}

fn scale_percent(scale_factor: f32) -> u16 {
  (scale_factor.max(1.0) * 100.0).round().clamp(100.0, 400.0) as u16
}

fn validate_resolution_scale(selected: u16, source: u16) -> Result<(), String> {
  if selected < 100 || selected > source {
    return Err("The selected output resolution is not available for this recording".to_owned());
  }

  Ok(())
}

#[tauri::command]
pub async fn save_export(
  app: AppHandle,
  file_stem: String,
  enabled_stream_indices: Vec<usize>,
  collapse_audio: bool,
  compression: u8,
  resolution_scale_percent: u16,
) -> Result<Option<PathBuf>, String> {
  if compression > 4 {
    return Err("Compression must be between 0 and 4".to_owned());
  }
  let stem =
    sanitize_file_stem(&file_stem).ok_or_else(|| "That file name cannot be used".to_owned())?;
  let directory =
    current_directory(&app).ok_or_else(|| "There is nowhere to save this".to_owned())?;
  let artifact = take_artifact(&app).ok_or_else(|| "There is nothing to save".to_owned())?;
  let artifact_id = match &artifact {
    ExportArtifact::Screenshot { id, .. } | ExportArtifact::Recording { id, .. } => *id,
  };
  let cancelled = Arc::new(AtomicBool::new(false));
  {
    let state = app.state::<ExportState>();
    let mut active = state
      .active_export
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    if active.is_some() {
      *app
        .state::<ExportState>()
        .artifact
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(artifact);
      emit_snapshot(&app);
      return Err("Another export is already running".to_owned());
    }
    *active = Some(ActiveExportJob {
      artifact_id,
      cancelled: Arc::clone(&cancelled),
    });
  }

  let writing = directory.clone();
  let progress_app = app.clone();
  let job_cancellation = Arc::clone(&cancelled);
  // The artifact travels back with an error. Saving used to take it before the
  // write and lose the export window's source when a disk or mux operation
  // failed, which made Retry impossible even though the recording remained on
  // disk.
  let (result, artifact) = tauri::async_runtime::spawn_blocking(move || {
    let result = (|| -> Result<Option<PathBuf>, String> {
      std::fs::create_dir_all(&writing).map_err(|error| error.to_string())?;

      match &artifact {
        ExportArtifact::Screenshot { image, .. } => {
          let path = unique_path(&writing, &stem, SCREENSHOT_EXTENSION, &|candidate| {
            candidate.exists()
          });
          std::fs::write(&path, encode_png(image)?).map_err(|error| error.to_string())?;
          Ok(Some(path))
        }
        ExportArtifact::Recording {
          audio_tracks,
          id,
          path: working,
          source_scale_percent,
          ..
        } => {
          validate_resolution_scale(resolution_scale_percent, *source_scale_percent)?;
          let selection =
            track_selection::TrackSelection::new(audio_tracks, &enabled_stream_indices);
          let layout = if collapse_audio {
            track_selection::AudioLayout::Mixdown
          } else {
            track_selection::AudioLayout::SeparateTracks
          };

          if compression > 0
            || resolution_scale_percent < *source_scale_percent
            || selection.needs_processing(audio_tracks, layout)
          {
            let mut on_progress = |processed_ms| {
              let _ = progress_app.emit(
                EXPORT_PROGRESS_EVENT,
                ExportProgress {
                  artifact_id: *id,
                  processed_ms,
                },
              );
            };
            save_selected_recording(
              working,
              &writing,
              &stem,
              &selection,
              layout,
              media_preview::ExportRunOptions {
                cancelled: &job_cancellation,
                on_progress: &mut on_progress,
                video: media_preview::VideoExportOptions {
                  compression,
                  resolution_scale_percent,
                  source_scale_percent: *source_scale_percent,
                },
              },
              media_preview::selected_recording_exporter(),
            )
          } else {
            save_recording(working, &writing, &stem, media_preview::remuxer()).map(Some)
          }
        }
      }
    })();

    (result, artifact)
  })
  .await
  .map_err(|error| error.to_string())?;

  {
    let state = app.state::<ExportState>();
    let mut active = state
      .active_export
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    if active
      .as_ref()
      .is_some_and(|job| job.artifact_id == artifact_id)
    {
      active.take();
    }
  }

  let path = match result {
    Ok(Some(path)) => path,
    Ok(None) => {
      *app
        .state::<ExportState>()
        .artifact
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(artifact);
      emit_snapshot(&app);
      return Ok(None);
    }
    Err(error) => {
      *app
        .state::<ExportState>()
        .artifact
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(artifact);
      emit_snapshot(&app);
      return Err(error);
    }
  };

  // Only remembered once a save actually lands there.
  set_export_directory(app.clone(), directory)?;
  let _ = window::hide(&app);
  emit_snapshot(&app);

  Ok(Some(path))
}

/// What to do with the recordings found in the working directory at startup.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct OrphanPlan {
  pub delete: Vec<PathBuf>,
  pub present: Option<PathBuf>,
}

/// Decides the fate of every recording left behind by a previous run.
///
/// A recording is only ever in the working directory because it was never
/// saved - the app quit, or crashed, between finishing and saving. The most
/// recent one is worth offering back, because it is almost certainly the one
/// that was on screen when that happened. Anything past its keeping age goes,
/// including the newest, so a machine that crashed a month ago does not
/// resurrect a recording nobody remembers making.
pub fn orphan_plan(entries: Vec<(PathBuf, SystemTime)>, now: SystemTime) -> OrphanPlan {
  let (fresh, stale): (Vec<_>, Vec<_>) = entries.into_iter().partition(|(_, modified)| {
    now
      .duration_since(*modified)
      .is_ok_and(|age| age <= ORPHAN_MAX_AGE)
      // A file stamped in the future has no believable age; keeping it is the
      // safer half of the guess.
      || modified > &now
  });

  OrphanPlan {
    delete: stale.into_iter().map(|(path, _)| path).collect(),
    present: fresh
      .into_iter()
      .max_by_key(|(_, modified)| *modified)
      .map(|(path, _)| path),
  }
}

fn orphaned_recordings(directory: &Path) -> Vec<(PathBuf, SystemTime)> {
  let Ok(entries) = std::fs::read_dir(directory) else {
    return Vec::new();
  };

  entries
    .filter_map(|entry| {
      let path = entry.ok()?.path();
      // A mixed preview is an `.mp4` in this same folder, and offering one
      // back as an unsaved recording would hand the user a derivative in place
      // of what they actually recorded.
      if media_preview::is_preview_file(&path) {
        return None;
      }
      let extension = path.extension()?;
      if WORKING_RECORDING_EXTENSIONS
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
      {
        Some((
          path.clone(),
          std::fs::metadata(&path).ok()?.modified().ok()?,
        ))
      } else {
        None
      }
    })
    .collect()
}

/// Offers back the recording an earlier run never got to save.
///
/// Deliberately not the whole artifact: the poster and the duration lived in
/// the frames, which are long gone. The name and the file are what matter, and
/// the window renders without a poster rather than pretending to have one.
/// Deletes every preview derivative left in the working directory.
///
/// They exist only for as long as an artifact is on screen, so at startup
/// there is no such thing as one worth keeping: any that are there were
/// stranded by a crash, and each is a copy of a movie sitting in the app's own
/// data directory where nobody will ever look for it.
///
/// The match is on the name's prefix rather than its extension, which is what
/// makes it reach the `.part` files a mix encodes into as well: those are
/// named after the mix they were going to become, so an encode killed halfway
/// is reclaimed here without this needing to know anything about it.
fn sweep_preview_files(directory: &Path) {
  let Ok(entries) = std::fs::read_dir(directory) else {
    return;
  };

  for entry in entries.flatten() {
    let path = entry.path();
    if media_preview::is_preview_file(&path) {
      let _ = std::fs::remove_file(path);
    }
  }
}

fn sweep_orphaned_recordings(app: &AppHandle) {
  let Ok(directory) = crate::recording::recordings_directory(app) else {
    return;
  };
  sweep_preview_files(&directory);
  let plan = orphan_plan(orphaned_recordings(&directory), SystemTime::now());

  for path in plan.delete {
    let _ = std::fs::remove_file(path);
  }
  let Some(path) = plan.present else {
    return;
  };

  let recorded_at = std::fs::metadata(&path)
    .and_then(|metadata| metadata.modified())
    .map_or_else(
      |_| chrono::Local::now(),
      chrono::DateTime::<chrono::Local>::from,
    );
  let suggested_file_stem = crate::screenshots::capture_file_stem(recorded_at.naive_local());
  if let Err(error) = present_recording(
    app,
    FinalizeInfo {
      has_microphone: false,
      has_system_audio: false,
      duration_ms: 0,
      height: 0,
      path,
      poster: None,
      source_scale_factor: 1.0,
      width: 0,
    },
    suggested_file_stem,
  ) {
    eprintln!("Could not offer back an unsaved recording: {error}");
  }
}

pub fn initialize(app: &AppHandle) {
  if let Some(directory) = load_directory(app) {
    *app
      .state::<ExportState>()
      .directory
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(directory);
  }

  sweep_orphaned_recordings(app);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn keeps_a_reasonable_name_untouched() {
    assert_eq!(
      sanitize_file_stem("Orbit Capture 2026-08-08 at 14.32.05").as_deref(),
      Some("Orbit Capture 2026-08-08 at 14.32.05")
    );
  }

  #[test]
  fn strips_characters_neither_platform_allows() {
    assert_eq!(
      sanitize_file_stem(r#"a<b>c:d"e/f\g|h?i*j"#).as_deref(),
      Some("abcdefghij")
    );
  }

  #[test]
  fn strips_control_characters() {
    assert_eq!(
      sanitize_file_stem("one\ttwo\nthree").as_deref(),
      Some("onetwothree")
    );
  }

  #[test]
  fn trims_surrounding_whitespace() {
    assert_eq!(sanitize_file_stem("   shot   ").as_deref(), Some("shot"));
  }

  #[test]
  fn drops_trailing_dots_and_spaces_that_windows_would_eat() {
    assert_eq!(sanitize_file_stem("shot. . .").as_deref(), Some("shot"));
    assert_eq!(sanitize_file_stem("shot   ").as_deref(), Some("shot"));
  }

  #[test]
  fn rejects_a_name_with_nothing_left_in_it() {
    assert_eq!(sanitize_file_stem(""), None);
    assert_eq!(sanitize_file_stem("   "), None);
    assert_eq!(sanitize_file_stem("///"), None);
    assert_eq!(sanitize_file_stem("..."), None);
  }

  #[test]
  fn rejects_names_windows_reserves() {
    assert_eq!(sanitize_file_stem("CON"), None);
    assert_eq!(sanitize_file_stem("nul"), None);
    assert_eq!(sanitize_file_stem("Com1"), None);
    assert_eq!(sanitize_file_stem("LPT9"), None);
    // Only the exact stem is reserved.
    assert_eq!(sanitize_file_stem("console").as_deref(), Some("console"));
  }

  #[test]
  fn caps_an_absurdly_long_name() {
    let stem = sanitize_file_stem(&"a".repeat(500)).unwrap();
    assert_eq!(stem.len(), MAX_FILE_STEM);
  }

  #[test]
  fn tells_a_preview_from_a_recording_however_far_it_got() {
    let directory = std::env::temp_dir()
      .join("orbit-capture-tests")
      .join("preview-sweep");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();

    let recording = directory.join("recording-20260808-143205.000.mov");
    let mix = directory.join("preview-42-7-mix-0-1.mp4");
    let abandoned = directory.join("preview-42-7-mix-0-1.mp4.3.part");
    for path in [&recording, &mix, &abandoned] {
      std::fs::write(path, b"movie").unwrap();
    }

    // Neither derivative may be offered back as the recording an earlier run
    // never saved - one is a mixdown, the other is not even a whole file.
    let orphans = orphaned_recordings(&directory);
    assert_eq!(
      orphans.iter().map(|(path, _)| path).collect::<Vec<_>>(),
      vec![&recording]
    );

    // Both go at startup, though: an interrupted encode is as worthless as a
    // finished mix once the artifact it belonged to is gone.
    sweep_preview_files(&directory);
    assert!(recording.is_file());
    assert!(!mix.exists());
    assert!(!abandoned.exists());
  }

  /// A directory of this test module's own, so a test that writes files cannot
  /// be confused by anything else on the machine.
  fn test_directory(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join("orbit-capture-tests").join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
  }

  #[test]
  fn recovers_an_unsaved_recording_whichever_container_it_was_written_in() {
    let directory = test_directory("orphan-containers");

    // What this version writes, and what the version before it wrote. Someone
    // who upgraded with an unsaved recording still on disk has the second.
    let quicktime = directory.join("recording-20260808-143205.000.mov");
    let legacy = directory.join("recording-20260807-091500.000.mp4");
    // Case is the file system's business, not ours.
    let shouted = directory.join("recording-20260806-091500.000.MOV");
    let unrelated = directory.join("notes.txt");
    for path in [&quicktime, &legacy, &shouted, &unrelated] {
      std::fs::write(path, b"movie").unwrap();
    }

    let mut found: Vec<PathBuf> = orphaned_recordings(&directory)
      .into_iter()
      .map(|(path, _)| path)
      .collect();
    found.sort();
    let mut expected = vec![quicktime, legacy, shouted];
    expected.sort();

    assert_eq!(found, expected);
  }

  #[test]
  fn describes_a_recording_by_the_file_the_user_will_actually_get() {
    let working = Path::new("/tmp/recording-20260808-143205.000.mov");
    assert_eq!(delivered_extension(working, true), "mp4");
    // Nothing to copy it with, so what is offered is the movie itself - never
    // that movie under a name it does not answer to.
    assert_eq!(delivered_extension(working, false), "mov");
    // A recording recovered from a version that wrote .mp4 is already what it
    // would have been remuxed into.
    assert_eq!(
      delivered_extension(Path::new("/tmp/recording-1.mp4"), false),
      "mp4"
    );
  }

  /// Stands in for a stream copy that works, without needing FFmpeg to be on
  /// the machine running the test.
  fn copies(source: &Path, destination: &Path) -> Result<(), String> {
    std::fs::copy(source, destination)
      .map(|_| ())
      .map_err(|error| error.to_string())
  }

  fn refuses(_: &Path, _: &Path) -> Result<(), String> {
    Err("no".to_owned())
  }

  fn copies_selected(
    source: &Path,
    destination: &Path,
    _: &track_selection::TrackSelection,
    _: track_selection::AudioLayout,
    _: media_preview::ExportRunOptions<'_>,
  ) -> Result<media_preview::ExportRunResult, String> {
    copies(source, destination).map(|()| media_preview::ExportRunResult::Completed)
  }

  fn refuses_selected(
    _: &Path,
    _: &Path,
    _: &track_selection::TrackSelection,
    _: track_selection::AudioLayout,
    _: media_preview::ExportRunOptions<'_>,
  ) -> Result<media_preview::ExportRunResult, String> {
    Err("no".to_owned())
  }

  #[test]
  fn saves_a_recording_as_an_mp4_when_it_can_be_copied_into_one() {
    let directory = test_directory("save-remuxed");
    let working = directory.join("recording-20260808-143205.000.mov");
    std::fs::write(&working, b"movie").unwrap();

    let saved = save_recording(&working, &directory, "Keeper", Some(copies)).unwrap();

    assert_eq!(saved, directory.join("Keeper.mp4"));
    assert!(saved.is_file());
    // The working file is let go of only once its replacement exists.
    assert!(!working.exists());
  }

  #[test]
  fn saves_a_recording_as_the_movie_it_is_when_there_is_nothing_to_copy_it_with() {
    let directory = test_directory("save-without-ffmpeg");
    let working = directory.join("recording-20260808-143205.000.mov");
    std::fs::write(&working, b"movie").unwrap();

    let saved = save_recording(&working, &directory, "Keeper", None).unwrap();

    // A .mov named .mp4 is a file that lies about itself, so the honest name
    // is the one the user gets.
    assert_eq!(saved, directory.join("Keeper.mov"));
    assert!(saved.is_file());
    assert!(!directory.join("Keeper.mp4").exists());
    assert!(!working.exists());
  }

  #[test]
  fn saves_a_recording_as_the_movie_it_is_when_the_copy_fails() {
    let directory = test_directory("save-failed-remux");
    let working = directory.join("recording-20260808-143205.000.mov");
    std::fs::write(&working, b"movie").unwrap();

    let saved = save_recording(&working, &directory, "Keeper", Some(refuses)).unwrap();

    // FFmpeg refusing the file is no reason to lose a recording someone just
    // asked to keep.
    assert_eq!(saved, directory.join("Keeper.mov"));
    assert!(saved.is_file());
    assert!(!directory.join("Keeper.mp4").exists());
  }

  #[test]
  fn saves_beside_a_name_that_is_already_taken() {
    let directory = test_directory("save-collision");
    let working = directory.join("recording-20260808-143205.000.mov");
    std::fs::write(&working, b"movie").unwrap();
    std::fs::write(directory.join("Keeper.mp4"), b"someone else's").unwrap();

    let saved = save_recording(&working, &directory, "Keeper", Some(copies)).unwrap();

    assert_eq!(saved, directory.join("Keeper (2).mp4"));
  }

  #[test]
  fn saves_a_selected_audio_layout_without_changing_the_working_movie() {
    let directory = test_directory("save-selected-audio");
    let working = directory.join("recording-20260808-143205.000.mov");
    std::fs::write(&working, b"movie").unwrap();
    let tracks = recording_audio_tracks(true, true);
    let selection = track_selection::TrackSelection::new(&tracks, &[1]);
    let cancelled = AtomicBool::new(false);
    let mut ignore_progress = |_| {};

    let saved = save_selected_recording(
      &working,
      &directory,
      "Keeper",
      &selection,
      track_selection::AudioLayout::SeparateTracks,
      media_preview::ExportRunOptions {
        cancelled: &cancelled,
        on_progress: &mut ignore_progress,
        video: media_preview::VideoExportOptions {
          compression: 0,
          resolution_scale_percent: 200,
          source_scale_percent: 200,
        },
      },
      Some(copies_selected),
    )
    .unwrap();

    assert_eq!(saved, Some(directory.join("Keeper.mp4")));
    assert!(!working.exists());
  }

  #[test]
  fn keeps_the_working_movie_when_a_selected_audio_export_fails() {
    let directory = test_directory("save-selected-audio-failure");
    let working = directory.join("recording-20260808-143205.000.mov");
    std::fs::write(&working, b"movie").unwrap();
    let tracks = recording_audio_tracks(true, true);
    let selection = track_selection::TrackSelection::new(&tracks, &[1]);
    let cancelled = AtomicBool::new(false);
    let mut ignore_progress = |_| {};

    assert!(save_selected_recording(
      &working,
      &directory,
      "Keeper",
      &selection,
      track_selection::AudioLayout::SeparateTracks,
      media_preview::ExportRunOptions {
        cancelled: &cancelled,
        on_progress: &mut ignore_progress,
        video: media_preview::VideoExportOptions {
          compression: 0,
          resolution_scale_percent: 200,
          source_scale_percent: 200,
        },
      },
      Some(refuses_selected),
    )
    .is_err());
    assert!(working.exists());
    assert!(!directory.join("Keeper.mp4").exists());
  }

  /// The one test here that uses the real stream copy. It is skipped rather
  /// than failed on a machine without FFmpeg, because that machine is exactly
  /// the one the fallback above exists for.
  #[test]
  fn carries_every_recorded_track_into_the_saved_mp4() {
    let Some(remux) = media_preview::remuxer() else {
      eprintln!("skipped: FFmpeg is not on this machine");
      return;
    };

    let directory = test_directory("save-real-remux");
    let working = directory.join("recording-20260808-143205.000.mov");
    // A picture and two audio tracks, which is what a recording with both
    // system audio and a microphone carries.
    let built = std::process::Command::new("ffmpeg")
      .args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "lavfi",
        "-i",
        "testsrc=size=320x240:rate=30:duration=1",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=1",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=880:duration=1",
        "-c:v",
        "libx264",
        "-c:a",
        "aac",
        "-map",
        "0:v",
        "-map",
        "1:a",
        "-map",
        "2:a",
      ])
      .arg(&working)
      .status();
    if !built.is_ok_and(|status| status.success()) {
      eprintln!("skipped: this FFmpeg could not build the source movie");
      return;
    }

    let saved = save_recording(&working, &directory, "Keeper", Some(remux)).unwrap();
    assert_eq!(saved, directory.join("Keeper.mp4"));

    // Three streams in, three streams out. Dropping the second audio track
    // here would be silent data loss, which is the whole reason the copy maps
    // every stream rather than the first of each kind.
    assert_eq!(streams(&saved), 3);
  }

  /// How many streams a file holds, read out of what FFmpeg prints about it.
  fn streams(path: &Path) -> usize {
    let output = std::process::Command::new("ffmpeg")
      .args(["-hide_banner", "-nostdin", "-i"])
      .arg(path)
      .output()
      .unwrap();

    String::from_utf8_lossy(&output.stderr)
      .lines()
      .filter(|line| line.trim_start().starts_with("Stream #"))
      .count()
  }

  const NOW: SystemTime = SystemTime::UNIX_EPOCH;

  fn aged(name: &str, ago: Duration) -> (PathBuf, SystemTime) {
    (PathBuf::from(name), NOW - ago)
  }

  #[test]
  fn offers_back_the_newest_unsaved_recording() {
    let plan = orphan_plan(
      vec![
        aged("/tmp/old.mov", Duration::from_secs(3_600)),
        aged("/tmp/newest.mov", Duration::from_secs(60)),
        aged("/tmp/middle.mov", Duration::from_secs(600)),
      ],
      NOW,
    );

    assert_eq!(plan.present.as_deref(), Some(Path::new("/tmp/newest.mov")));
    // Still inside their keeping age, so a later run can still offer them.
    assert!(plan.delete.is_empty());
  }

  #[test]
  fn sweeps_away_anything_past_its_keeping_age() {
    let plan = orphan_plan(
      vec![
        aged("/tmp/ancient.mov", ORPHAN_MAX_AGE + Duration::from_secs(1)),
        aged("/tmp/recent.mov", Duration::from_secs(60)),
      ],
      NOW,
    );

    assert_eq!(plan.delete, vec![PathBuf::from("/tmp/ancient.mov")]);
    assert_eq!(plan.present.as_deref(), Some(Path::new("/tmp/recent.mov")));
  }

  #[test]
  fn offers_nothing_back_when_everything_is_too_old() {
    let plan = orphan_plan(vec![aged("/tmp/ancient.mov", ORPHAN_MAX_AGE * 2)], NOW);

    assert_eq!(plan.present, None);
    assert_eq!(plan.delete.len(), 1);
  }

  #[test]
  fn keeps_a_recording_stamped_in_the_future() {
    // A clock that moved backwards makes an age impossible to read, and
    // deleting someone's recording is the worse half of that guess.
    let plan = orphan_plan(
      vec![(
        PathBuf::from("/tmp/ahead.mov"),
        NOW + Duration::from_secs(60),
      )],
      NOW,
    );

    assert_eq!(plan.present.as_deref(), Some(Path::new("/tmp/ahead.mov")));
    assert!(plan.delete.is_empty());
  }

  #[test]
  fn does_nothing_with_an_empty_directory() {
    assert_eq!(orphan_plan(Vec::new(), NOW), OrphanPlan::default());
  }

  #[test]
  fn keeps_a_dot_inside_the_name() {
    assert_eq!(
      sanitize_file_stem("v1.2.3 build").as_deref(),
      Some("v1.2.3 build")
    );
  }
}
