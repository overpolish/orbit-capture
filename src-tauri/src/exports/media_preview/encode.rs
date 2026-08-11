// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

mod args;

pub(super) use args::{audio_export_args, camera_export_args, remux_args, selected_export_args};

/// Counts the remuxes this process has attempted, so the temporary a save
/// writes through never collides with another save's.
static REMUX_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

/// Where the stream copy writes while it is still working.
///
/// FFmpeg only stamps the index that makes an MP4 playable when it finishes,
/// so until then the file on disk is a truncation. The final name is created
/// by a rename, which is atomic within a directory, so what the user goes
/// looking for is either absent or whole.
///
/// A sibling of the destination rather than a temp directory, so the rename
/// cannot cross a volume - the destination is wherever the user chose to save,
/// which is routinely an external disk.
pub(in crate::exports) fn remux_temp_path(destination: &Path) -> PathBuf {
  let attempt = REMUX_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
  let name = destination.file_name().map_or_else(
    || "recording".to_owned(),
    |name| name.to_string_lossy().into_owned(),
  );

  destination.with_file_name(format!(".{name}.{}.{attempt}.part", std::process::id()))
}

pub(in crate::exports) fn remux_error(stderr: &[u8]) -> String {
  const MESSAGE: &str = "FFmpeg could not put the recording into an MP4";
  let detail = String::from_utf8_lossy(stderr);
  let detail = detail.trim();
  if detail.is_empty() {
    return MESSAGE.to_owned();
  }

  let tail = detail
    .char_indices()
    .rev()
    .nth(OUTPUT_ERROR_DETAIL - 1)
    .map_or(detail, |(index, _)| &detail[index..]);

  format!("{MESSAGE}: {tail}")
}

/// Stream-copies `source` into `destination`, leaving nothing behind if it
/// cannot. `source` is untouched either way - the caller decides when the
/// working file has been superseded.
pub fn remux(source: &Path, destination: &Path) -> Result<(), String> {
  let temporary = remux_temp_path(destination);
  let output = Command::new(ffmpeg_path())
    .args(remux_args(source, &temporary))
    .output()
    .map_err(|error| {
      let _ = std::fs::remove_file(&temporary);
      format!("FFmpeg could not be started: {error}")
    })?;

  // A failed copy, an empty file and an unopenable one all mean the same
  // thing: the working movie is still the only real recording, so the caller
  // must be told to save that instead.
  if !output.status.success() || !holds_bytes(&temporary) || !plays_from_start_to_end(&temporary) {
    let _ = std::fs::remove_file(&temporary);
    return Err(remux_error(&output.stderr));
  }

  std::fs::rename(&temporary, destination).map_err(|error| {
    let _ = std::fs::remove_file(&temporary);
    format!("The recording could not be put in place: {error}")
  })
}

/// Writes a saved movie with exactly the requested compression, audio streams
/// and layout. Unlike the ordinary remux, failure is returned to the export
/// window: silently falling back would produce a file unlike the one shown.
pub fn export_selected_recording(
  source: &Path,
  destination: &Path,
  selection: &TrackSelection,
  layout: AudioLayout,
  run: ExportRunOptions<'_>,
) -> Result<ExportRunResult, String> {
  let ExportRunOptions {
    cancelled,
    on_progress,
    video,
  } = run;
  let VideoExportOptions {
    compression,
    resolution_scale_percent,
    source_scale_percent,
  } = video;
  if (compression > 0 || resolution_scale_percent < source_scale_percent) && !supports_compression()
  {
    return Err("This FFmpeg build does not include the H.264 encoder".to_owned());
  }
  let temporary = remux_temp_path(destination);
  let args = selected_export_args(source, &temporary, selection, layout, video);
  run_export(args, &temporary, destination, cancelled, on_progress)
}

pub fn export_camera_recording(
  audio_source: &Path,
  camera_source: &Path,
  destination: &Path,
  selection: &TrackSelection,
  layout: AudioLayout,
  run: ExportRunOptions<'_>,
) -> Result<ExportRunResult, String> {
  let ExportRunOptions {
    cancelled,
    on_progress,
    video,
  } = run;
  if (video.compression > 0 || video.resolution_scale_percent < video.source_scale_percent)
    && !supports_compression()
  {
    return Err("This FFmpeg build does not include the H.264 encoder".to_owned());
  }
  let temporary = remux_temp_path(destination);
  let args = camera_export_args(
    audio_source,
    camera_source,
    &temporary,
    selection,
    layout,
    video,
  );
  run_export(args, &temporary, destination, cancelled, on_progress)
}

pub fn export_selected_audio(
  source: &Path,
  destination: &Path,
  selection: &TrackSelection,
  layout: AudioLayout,
  run: ExportRunOptions<'_>,
) -> Result<ExportRunResult, String> {
  let ExportRunOptions {
    cancelled,
    on_progress,
    video: _,
  } = run;
  let temporary = remux_temp_path(destination);
  let args = audio_export_args(source, &temporary, selection, layout);
  run_export(args, &temporary, destination, cancelled, on_progress)
}

pub(in crate::exports) fn run_export(
  args: Vec<OsString>,
  temporary: &Path,
  destination: &Path,
  cancelled: &AtomicBool,
  on_progress: &mut dyn FnMut(u64),
) -> Result<ExportRunResult, String> {
  let mut child = Command::new(ffmpeg_path())
    .args(args)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|error| {
      let _ = std::fs::remove_file(temporary);
      format!("FFmpeg could not be started: {error}")
    })?;

  let stderr = child
    .stderr
    .take()
    .ok_or_else(|| "FFmpeg did not expose its error output".to_owned())?;
  let stderr_reader = std::thread::spawn(move || {
    let mut bytes = Vec::new();
    let _ = BufReader::new(stderr).read_to_end(&mut bytes);
    bytes
  });
  let stdout = child
    .stdout
    .take()
    .ok_or_else(|| "FFmpeg did not expose its progress output".to_owned())?;
  if cancelled.load(Ordering::Acquire) {
    let _ = child.kill();
  } else {
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
      if cancelled.load(Ordering::Acquire) {
        let _ = child.kill();
        break;
      }
      if let Some(milliseconds) = progress_milliseconds(&line) {
        on_progress(milliseconds);
      }
    }
  }
  // Covers a cancellation arriving after FFmpeg's final progress line but
  // before its process has been reaped.
  if cancelled.load(Ordering::Acquire) {
    let _ = child.kill();
  }
  let status = child.wait().map_err(|error| {
    let _ = std::fs::remove_file(temporary);
    format!("FFmpeg could not be completed: {error}")
  })?;
  let stderr = stderr_reader.join().unwrap_or_default();

  if cancelled.load(Ordering::Acquire) {
    let _ = std::fs::remove_file(temporary);
    return Ok(ExportRunResult::Cancelled);
  }

  if !status.success() || !holds_bytes(temporary) || !plays_from_start_to_end(temporary) {
    let _ = std::fs::remove_file(temporary);
    return Err(remux_error(&stderr));
  }

  std::fs::rename(temporary, destination).map_err(|error| {
    let _ = std::fs::remove_file(temporary);
    format!("The recording could not be put in place: {error}")
  })?;

  Ok(ExportRunResult::Completed)
}

/// Whether FFmpeg is on this machine at all, resolved once.
///
/// Read once per run rather than per save because it answers a question about
/// the machine, and because it is asked every time the export window is told
/// what is waiting for it - a process launch on that path would be felt.
/// Nothing depends on it being right: a save that is told FFmpeg is there and
/// finds otherwise falls back to keeping the QuickTime movie, and one told it
/// is missing simply keeps the movie without trying.
pub(super) fn ffmpeg_runs() -> bool {
  static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

  *AVAILABLE.get_or_init(|| {
    Command::new(ffmpeg_path())
      .args(["-hide_banner", "-version"])
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .status()
      .is_ok_and(|status| status.success())
  })
}

/// The stream copy a save uses to turn the working QuickTime movie into an
/// .mp4. A function pointer rather than a direct call, so the save path can be
/// driven - in a test - by a machine that has FFmpeg and by one that does not.
pub type Remux = fn(&Path, &Path) -> Result<(), String>;

/// The stream copy a save should use, or `None` on a machine without FFmpeg,
/// where the recording can only be handed over as the QuickTime movie it is.
pub fn remuxer() -> Option<Remux> {
  ffmpeg_runs().then_some(remux as Remux)
}

pub type SelectedRecordingExport = for<'a> fn(
  &Path,
  &Path,
  &TrackSelection,
  AudioLayout,
  ExportRunOptions<'a>,
) -> Result<ExportRunResult, String>;

pub type CameraRecordingExport = for<'a> fn(
  &Path,
  &Path,
  &Path,
  &TrackSelection,
  AudioLayout,
  ExportRunOptions<'a>,
) -> Result<ExportRunResult, String>;

pub type SelectedAudioExport = for<'a> fn(
  &Path,
  &Path,
  &TrackSelection,
  AudioLayout,
  ExportRunOptions<'a>,
) -> Result<ExportRunResult, String>;

pub(super) fn progress_milliseconds(line: &str) -> Option<u64> {
  line
    .strip_prefix("out_time_us=")?
    .parse::<u64>()
    .ok()
    .map(|microseconds| microseconds / 1_000)
}

/// The audio-aware export operation, if FFmpeg is available.
pub fn selected_recording_exporter() -> Option<SelectedRecordingExport> {
  ffmpeg_runs().then_some(export_selected_recording as SelectedRecordingExport)
}

pub fn camera_recording_exporter() -> Option<CameraRecordingExport> {
  ffmpeg_runs().then_some(export_camera_recording as CameraRecordingExport)
}

pub fn selected_audio_exporter() -> Option<SelectedAudioExport> {
  ffmpeg_runs().then_some(export_selected_audio as SelectedAudioExport)
}
