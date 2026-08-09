//! Audio prepared only for the export window.
//!
//! The recording remains the source of truth. FFmpeg decodes a low-rate mono
//! signal from each of its tracks for a waveform, and mixes the enabled ones
//! into a single file the window can play. Closing or saving the artifact
//! removes the mixes; none of them can become an export by accident.
//!
//! Each track was once also stream-copied into its own small M4A. Nothing ever
//! played them - the waveforms are decoded straight from the recording and the
//! window plays the mix - so they were an FFmpeg pass and a file per track for
//! no reader at all.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use serde::Serialize;

use super::track_selection::{AudioLayout, TrackSelection};
use super::{AudioTrackKind, RecordingAudioTrack};

const WAVEFORM_POINTS: usize = 512;
const WAVEFORM_SAMPLE_RATE: u64 = 8_000;
/// Every file this module writes starts with it. Nothing else in the
/// recordings directory does, which is what lets both the cleanup paths and
/// the startup sweep tell a derivative from a recording by its name alone.
pub const PREVIEW_PREFIX: &str = "preview-";

/// Whether a path is one of this module's derivatives rather than a recording.
pub fn is_preview_file(path: &Path) -> bool {
  path
    .file_name()
    .and_then(|name| name.to_str())
    .is_some_and(|name| name.starts_with(PREVIEW_PREFIX))
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedAudioTrack {
  pub kind: AudioTrackKind,
  pub label: String,
  /// Which recorded track this describes, so the window can name it back when
  /// it asks for a mix. Also what identifies the row on screen.
  pub stream_index: usize,
  pub waveform: Vec<f32>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingPreview {
  pub artifact_id: u64,
  pub tracks: Vec<PreparedAudioTrack>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoExportOptions {
  pub compression: u8,
  pub resolution_scale_percent: u16,
  pub source_scale_percent: u16,
}

pub struct ExportRunOptions<'a> {
  pub cancelled: &'a AtomicBool,
  pub on_progress: &'a mut dyn FnMut(u64),
  pub video: VideoExportOptions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportRunResult {
  Completed,
  Cancelled,
}

/// Where a package manager puts FFmpeg and its siblings. A bundled app
/// inherits its environment from launchd rather than from a shell, so `PATH`
/// alone finds nothing that was installed by Homebrew or MacPorts.
#[cfg(not(windows))]
const TOOL_SEARCH_DIRECTORIES: &[&str] = &[
  "/opt/homebrew/bin",
  "/usr/local/bin",
  "/opt/local/bin",
  "/usr/bin",
];

#[cfg(windows)]
const TOOL_SEARCH_DIRECTORIES: &[&str] = &[];

fn tool_path(name: &str) -> PathBuf {
  let executable = if cfg!(windows) {
    format!("{name}.exe")
  } else {
    name.to_owned()
  };
  // Alongside the app first: a bundled copy is the one this build was tested
  // against.
  if let Ok(current) = std::env::current_exe() {
    if let Some(directory) = current.parent() {
      let bundled = directory.join(&executable);
      if bundled.is_file() {
        return bundled;
      }
    }
  }
  for directory in TOOL_SEARCH_DIRECTORIES {
    let path = Path::new(directory).join(&executable);
    if path.is_file() {
      return path;
    }
  }

  PathBuf::from(executable)
}

fn ffmpeg_path() -> PathBuf {
  tool_path("ffmpeg")
}

/// FFprobe is only ever used to *check* a file this module just wrote, never
/// to produce one. Everything here still works without it - see
/// [`plays_from_start_to_end`] for what is given up when it is missing.
fn ffprobe_path() -> PathBuf {
  tool_path("ffprobe")
}

pub fn inspect_audio_tracks(source: &Path) -> Result<Vec<RecordingAudioTrack>, String> {
  // FFmpeg prints container metadata before it complains that no output was
  // supplied. That gives recovery everything it needs without shipping a
  // second, almost equally large FFprobe executable.
  let output = Command::new(ffmpeg_path())
    .args(["-hide_banner", "-nostdin", "-i"])
    .arg(source)
    .output()
    .map_err(|error| format!("FFmpeg could not be started: {error}"))?;
  let metadata = String::from_utf8_lossy(&output.stderr);
  let count = metadata
    .lines()
    .filter(|line| line.contains("Stream #") && line.contains(" Audio:"))
    .count();

  Ok(
    (0..count)
      .map(|stream_index| RecordingAudioTrack {
        kind: AudioTrackKind::Unknown,
        label: format!("Audio {}", stream_index + 1),
        stream_index,
      })
      .collect(),
  )
}

fn waveform(
  source: &Path,
  track: &RecordingAudioTrack,
  duration_ms: u64,
) -> Result<Vec<f32>, String> {
  let mut child = Command::new(ffmpeg_path())
    .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-i"])
    .arg(source)
    .args([
      "-map",
      &format!("0:a:{}", track.stream_index),
      "-vn",
      "-ac",
      "1",
      "-ar",
      &WAVEFORM_SAMPLE_RATE.to_string(),
      "-f",
      "f32le",
      "pipe:1",
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|error| format!("FFmpeg could not be started: {error}"))?;

  let stdout = child
    .stdout
    .take()
    .ok_or_else(|| "FFmpeg did not expose its waveform output".to_owned())?;
  let expected_samples = duration_ms
    .saturating_mul(WAVEFORM_SAMPLE_RATE)
    .div_ceil(1_000)
    .max(1);
  let mut peaks = vec![0.0_f32; WAVEFORM_POINTS];
  let mut reader = BufReader::new(stdout);
  let mut bytes = [0_u8; 16 * 1024];
  let mut remainder = Vec::with_capacity(3);
  let mut sample_index = 0_u64;

  loop {
    let read = reader.read(&mut bytes).map_err(|error| error.to_string())?;
    if read == 0 {
      break;
    }
    remainder.extend_from_slice(&bytes[..read]);
    let complete = remainder.len() / 4 * 4;
    for sample in remainder[..complete].chunks_exact(4) {
      let value = f32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]);
      let bucket = ((sample_index.saturating_mul(WAVEFORM_POINTS as u64)) / expected_samples)
        .min((WAVEFORM_POINTS - 1) as u64) as usize;
      peaks[bucket] = peaks[bucket].max(value.abs().min(1.0));
      sample_index = sample_index.saturating_add(1);
    }
    remainder.drain(..complete);
  }

  let output = child
    .wait_with_output()
    .map_err(|error| error.to_string())?;
  if !output.status.success() {
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    return Err(if detail.is_empty() {
      format!("FFmpeg could not read the {} waveform", track.label)
    } else {
      detail
    });
  }

  Ok(peaks)
}

pub fn prepare(
  artifact_id: u64,
  source: &Path,
  duration_ms: u64,
  tracks: &[RecordingAudioTrack],
) -> Result<RecordingPreview, String> {
  let mut prepared = Vec::with_capacity(tracks.len());
  for track in tracks {
    // Nothing is written, so a failure part-way through leaves nothing behind
    // to tidy up - only a preview the window will not show.
    prepared.push(PreparedAudioTrack {
      kind: track.kind,
      label: track.label.clone(),
      stream_index: track.stream_index,
      waveform: waveform(source, track, duration_ms)?,
    });
  }

  Ok(RecordingPreview {
    artifact_id,
    tracks: prepared,
  })
}

/// Names a mix so that only the process that wrote it ever writes to it.
///
/// Artifact ids are minted from a counter that starts at one in every process,
/// so a second copy of the app - sharing the same recordings directory -
/// numbers its first recording `1` as well and would otherwise arrive at the
/// identical file name. Both would then be encoding onto one path while the
/// other's window played it, which is precisely how a preview ends up with a
/// band of garbage across it and a playhead that stops moving. The process id
/// makes each instance's mixes its own, and both remain reclaimable at startup
/// because the name still begins with the preview prefix.
fn mix_path(source: &Path, artifact_id: u64, signature: &str) -> PathBuf {
  mix_path_for(source, std::process::id(), artifact_id, signature)
}

fn mix_path_for(source: &Path, process: u32, artifact_id: u64, signature: &str) -> PathBuf {
  source.with_file_name(format!(
    "{PREVIEW_PREFIX}{process}-{artifact_id}-mix-{signature}.mp4"
  ))
}

/// Counts the mixes this process has attempted, so two of them in flight at
/// once - or one retried after a failure - never share a half-written file.
static MIX_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

/// Where FFmpeg writes while it is still working.
///
/// FFmpeg fills an MP4 progressively and only stamps the index that makes it
/// playable when it finishes, so for the whole length of the encode the file
/// on disk is a truncation. Writing that under the name the window is told to
/// play hands it whatever existed at the moment it looked. The final name is
/// only ever created by a rename, which is atomic within a directory, so the
/// path either does not exist or is a finished file - never something in
/// between.
///
/// The name is a sibling of the destination and inherits its process id, plus
/// an attempt number of its own; the preview prefix carries over from the
/// destination, so the startup sweep reclaims a `.part` left by a crash just
/// as it reclaims a finished mix.
fn mix_temp_path(destination: &Path) -> PathBuf {
  let attempt = MIX_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
  let name = destination.file_name().map_or_else(
    || PREVIEW_PREFIX.to_owned(),
    |name| name.to_string_lossy().into_owned(),
  );

  destination.with_file_name(format!("{name}.{attempt}.part"))
}

/// Writes a single file the export window can simply play.
///
/// The picture is stream-copied - always, without exception. This runs on a
/// recording the user is about to keep, and re-encoding it here would cost
/// minutes and quality to produce something nobody keeps. Only the audio is
/// touched, and only because a video element can play one track at a time.
fn mix_args(source: &Path, destination: &Path, selection: &TrackSelection) -> Vec<OsString> {
  let mut args: Vec<OsString> = ["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"]
    .map(OsString::from)
    .into();
  args.push(source.into());
  args.extend(["-map", "0:v:0", "-c:v", "copy"].map(OsString::from));
  args.extend(
    selection
      .audio_args(AudioLayout::Mixdown)
      .into_iter()
      .map(OsString::from),
  );
  args.extend(PREVIEW_MP4_OUTPUT.map(OsString::from));
  args.push(destination.into());

  args
}

/// What a preview mix is, spelled out rather than left to be guessed. FFmpeg
/// cannot infer a muxer from its temporary `.part` name. `+faststart` puts the
/// index first; signed composition offsets stop it compensating for H.264
/// reordering with a video edit that skips the decoder's initial references.
const PREVIEW_MP4_OUTPUT: [&str; 4] = ["-f", "mp4", "-movflags", "+faststart+negative_cts_offsets"];

/// The saved movie already plays reliably across the supported players, so it
/// deliberately keeps its established muxing flags. The signed composition
/// offsets above are a WebKit preview compatibility measure, not a reason to
/// change a known-good export container.
const EXPORT_MP4_OUTPUT: [&str; 4] = ["-f", "mp4", "-movflags", "+faststart"];

/// How much of FFmpeg's complaint is worth carrying back to the window. The
/// interesting line is always the last one; everything before it is context
/// the user cannot act on.
const MIX_ERROR_DETAIL: usize = 400;

fn mix_error(stderr: &[u8]) -> String {
  const MESSAGE: &str = "FFmpeg could not prepare the preview audio";
  let detail = String::from_utf8_lossy(stderr);
  let detail = detail.trim();
  if detail.is_empty() {
    return MESSAGE.to_owned();
  }

  let tail = detail
    .char_indices()
    .rev()
    .nth(MIX_ERROR_DETAIL - 1)
    .map_or(detail, |(index, _)| &detail[index..]);

  format!("{MESSAGE}: {tail}")
}

/// Whether a finished encode is a file a player can actually open.
///
/// A truncated MP4 has no index, so a probe of its container fails where a
/// look at its size would not - which is the whole point of asking. FFprobe is
/// not a hard dependency, though: if it cannot be started at all, the mix is
/// taken on the size check alone rather than refusing to produce a preview on
/// a machine that only has the encoder.
fn plays_from_start_to_end(path: &Path) -> bool {
  let Ok(output) = Command::new(ffprobe_path())
    .args([
      "-v",
      "error",
      "-show_entries",
      "format=duration",
      "-of",
      "default=noprint_wrappers=1:nokey=1",
    ])
    .arg(path)
    .output()
  else {
    return true;
  };

  output.status.success()
    && String::from_utf8_lossy(&output.stdout)
      .trim()
      .parse::<f64>()
      .is_ok_and(|duration| duration > 0.0)
}

/// Whether a file is there and has something in it. Cheap enough to run every
/// time a mix is handed out, unlike [`plays_from_start_to_end`].
fn holds_bytes(path: &Path) -> bool {
  std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
}

fn mix(source: &Path, destination: &Path, selection: &TrackSelection) -> Result<(), String> {
  let temporary = mix_temp_path(destination);
  // Captured rather than inherited: a bundled app has nowhere to print to, and
  // the one line FFmpeg writes when it gives up is the only thing that says
  // why the preview never appeared.
  let output = Command::new(ffmpeg_path())
    .args(mix_args(source, &temporary, selection))
    .output()
    .map_err(|error| {
      let _ = std::fs::remove_file(&temporary);
      format!("FFmpeg could not be started: {error}")
    })?;

  // A failed encode, an empty file and an unopenable one all mean the same
  // thing here: nothing worth publishing under the final name. The temporary
  // goes in every case, so a retry never inherits the last attempt's remains.
  if !output.status.success() || !holds_bytes(&temporary) || !plays_from_start_to_end(&temporary) {
    let _ = std::fs::remove_file(&temporary);
    return Err(mix_error(&output.stderr));
  }

  std::fs::rename(&temporary, destination).map_err(|error| {
    let _ = std::fs::remove_file(&temporary);
    format!("The preview audio could not be put in place: {error}")
  })?;

  Ok(())
}

/// Turns the working QuickTime movie into the .mp4 the user keeps.
///
/// Not a re-encode and not a rename. `-c copy` on `-map 0` carries every
/// stream across untouched - there can be two audio tracks, and dropping the
/// microphone here would be silent data loss - so this costs a read and a
/// write of the file rather than minutes of encoding.
///
/// The recording is written as a QuickTime movie because that is the only
/// container that survives being fragmented, and fragments are what make a
/// crashed recording recoverable. Nothing about that is the user's problem, so
/// what they keep is the .mp4 everything opens.
fn remux_args(source: &Path, destination: &Path) -> Vec<OsString> {
  let mut args: Vec<OsString> = ["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"]
    .map(OsString::from)
    .into();
  args.push(source.into());
  args.extend(["-map", "0", "-c", "copy"].map(OsString::from));
  args.extend(EXPORT_MP4_OUTPUT.map(OsString::from));
  args.push(destination.into());

  args
}

/// FFmpeg arguments for an export that differs from the source. Video is
/// stream-copied at Original and quality-encoded at every compression level;
/// selected audio is decoded only when several tracks must become one.
fn selected_export_args(
  source: &Path,
  destination: &Path,
  selection: &TrackSelection,
  layout: AudioLayout,
  video: VideoExportOptions,
) -> Vec<OsString> {
  let VideoExportOptions {
    compression,
    resolution_scale_percent,
    source_scale_percent,
  } = video;
  let mut args: Vec<OsString> = ["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"]
    .map(OsString::from)
    .into();
  args.push(source.into());
  // Machine-readable progress belongs on the save encode, not on preview
  // mixes. It is written to stdout while diagnostics continue to use stderr.
  args.extend(["-progress", "pipe:1", "-nostats"].map(OsString::from));
  args.extend(["-map", "0:v:0?"].map(OsString::from));
  let scale_filter = resolution_filter(source_scale_percent, resolution_scale_percent);
  if let Some(crf) = export_crf(compression, scale_filter.is_some()) {
    args.extend(
      [
        "-c:v",
        "libx264",
        "-preset",
        "medium",
        "-crf",
        &crf.to_string(),
        "-pix_fmt",
        "yuv420p",
        "-profile:v",
        "high",
      ]
      .map(OsString::from),
    );
    if let Some(filter) = scale_filter {
      args.extend([OsString::from("-vf"), OsString::from(filter)]);
    }
  } else {
    args.extend(["-c:v", "copy"].map(OsString::from));
  }
  args.extend(selection.audio_args(layout).into_iter().map(OsString::from));
  args.extend(EXPORT_MP4_OUTPUT.map(OsString::from));
  args.push(destination.into());

  args
}

/// Counts the remuxes this process has attempted, so the temporary a save
/// writes through never collides with another save's.
static REMUX_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

/// Where the stream copy writes while it is still working.
///
/// Same discipline as [`mix_temp_path`], for the same reason: FFmpeg only
/// stamps the index that makes an MP4 playable when it finishes, so until then
/// the file on disk is a truncation. The final name is created by a rename,
/// which is atomic within a directory, so what the user goes looking for is
/// either absent or whole.
///
/// A sibling of the destination rather than a temp directory, so the rename
/// cannot cross a volume - the destination is wherever the user chose to save,
/// which is routinely an external disk.
fn remux_temp_path(destination: &Path) -> PathBuf {
  let attempt = REMUX_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
  let name = destination.file_name().map_or_else(
    || "recording".to_owned(),
    |name| name.to_string_lossy().into_owned(),
  );

  destination.with_file_name(format!(".{name}.{}.{attempt}.part", std::process::id()))
}

fn remux_error(stderr: &[u8]) -> String {
  const MESSAGE: &str = "FFmpeg could not put the recording into an MP4";
  let detail = String::from_utf8_lossy(stderr);
  let detail = detail.trim();
  if detail.is_empty() {
    return MESSAGE.to_owned();
  }

  let tail = detail
    .char_indices()
    .rev()
    .nth(MIX_ERROR_DETAIL - 1)
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
  let mut child = Command::new(ffmpeg_path())
    .args(selected_export_args(
      source, &temporary, selection, layout, video,
    ))
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|error| {
      let _ = std::fs::remove_file(&temporary);
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
    let _ = std::fs::remove_file(&temporary);
    format!("FFmpeg could not be completed: {error}")
  })?;
  let stderr = stderr_reader.join().unwrap_or_default();

  if cancelled.load(Ordering::Acquire) {
    let _ = std::fs::remove_file(&temporary);
    return Ok(ExportRunResult::Cancelled);
  }

  if !status.success() || !holds_bytes(&temporary) || !plays_from_start_to_end(&temporary) {
    let _ = std::fs::remove_file(&temporary);
    return Err(remux_error(&stderr));
  }

  std::fs::rename(&temporary, destination).map_err(|error| {
    let _ = std::fs::remove_file(&temporary);
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
fn ffmpeg_runs() -> bool {
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

fn progress_milliseconds(line: &str) -> Option<u64> {
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

/// The H.264 quality represented by the compression control.
///
/// Zero is deliberately not a CRF: it means the original encoded video is
/// copied without a generation loss. The remaining values are named quality
/// steps in the UI, so each maps to a stable encoder setting.
fn compression_crf(compression: u8) -> Option<u16> {
  match compression {
    0 => None,
    1 => Some(20),
    2 => Some(24),
    3 => Some(28),
    _ => Some(32),
  }
}

fn resolution_filter(source_scale_percent: u16, resolution_scale_percent: u16) -> Option<String> {
  (resolution_scale_percent < source_scale_percent).then(|| {
    format!(
      "scale=trunc(iw*{resolution_scale_percent}/{source_scale_percent}/2)*2:trunc(ih*{resolution_scale_percent}/{source_scale_percent}/2)*2:flags=lanczos"
    )
  })
}

/// Resizing cannot stream-copy. High quality is deliberately used if a caller
/// requests a smaller resolution with Original compression.
fn export_crf(compression: u8, is_resizing: bool) -> Option<u16> {
  compression_crf(compression).or(is_resizing.then_some(20))
}

/// Whether this FFmpeg build carries the software H.264 encoder used to make
/// compression behave the same on macOS and Windows.
pub fn supports_compression() -> bool {
  static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

  *AVAILABLE.get_or_init(|| {
    Command::new(ffmpeg_path())
      .args(["-hide_banner", "-encoders"])
      .stdin(Stdio::null())
      .output()
      .is_ok_and(|output| {
        output.status.success()
          && String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.split_whitespace().nth(1) == Some("libx264"))
      })
  })
}

static ESTIMATE_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

fn estimate_temp_path(source: &Path) -> PathBuf {
  let attempt = ESTIMATE_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
  source.with_file_name(format!(
    "{PREVIEW_PREFIX}estimate-{}-{attempt}.h264.part",
    std::process::id()
  ))
}

fn estimate_filter(sample_count: usize, scale_filter: Option<&str>) -> String {
  let mut filter = String::new();
  if sample_count == 1 {
    filter.push_str("[0:v:0]setpts=PTS-STARTPTS");
  } else {
    for index in 0..sample_count {
      filter.push_str(&format!("[{index}:v:0]setpts=PTS-STARTPTS[sample{index}];"));
    }
    for index in 0..sample_count {
      filter.push_str(&format!("[sample{index}]"));
    }
    filter.push_str(&format!("concat=n={sample_count}:v=1:a=0"));
  }
  if let Some(scale_filter) = scale_filter {
    filter.push(',');
    filter.push_str(scale_filter);
  }
  filter.push_str("[estimated]");

  filter
}

/// Estimates compressed video size from the start, middle and end rather than
/// assuming a screen is equally busy throughout. The seeked pieces are joined
/// before one encoder, so they pay the mandatory opening I-frame once rather
/// than once per sample. The output is raw H.264 so MP4 headers cannot be
/// multiplied into the estimate.
pub fn estimate_compressed_video_bytes(
  source: &Path,
  duration_ms: u64,
  compression: u8,
  source_scale_percent: u16,
  resolution_scale_percent: u16,
) -> Result<u64, String> {
  let scale_filter = resolution_filter(source_scale_percent, resolution_scale_percent);
  let crf = export_crf(compression, scale_filter.is_some())
    .ok_or_else(|| "Original video does not need a compression estimate".to_owned())?;
  if !supports_compression() {
    return Err("This FFmpeg build does not include the H.264 encoder".to_owned());
  }
  if duration_ms == 0 {
    return Err("The recording duration is not available".to_owned());
  }

  let duration = duration_ms as f64 / 1_000.0;
  let sample_count = if duration < 3.0 { 1 } else { 3 };
  let sample_duration = if sample_count == 1 { duration } else { 1.0 };
  let last_start = (duration - sample_duration).max(0.0);
  let starts = match sample_count {
    1 => vec![0.0],
    _ => vec![0.0, last_start / 2.0, last_start],
  };

  let temporary = estimate_temp_path(source);
  let mut command = Command::new(ffmpeg_path());
  command.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"]);
  for start in &starts {
    command
      .arg("-ss")
      .arg(format!("{start:.3}"))
      .arg("-t")
      .arg(format!("{sample_duration:.3}"))
      .arg("-i")
      .arg(source);
  }
  command
    .args([
      "-filter_complex",
      &estimate_filter(sample_count, scale_filter.as_deref()),
    ])
    .args([
      "-map",
      "[estimated]",
      "-an",
      "-c:v",
      "libx264",
      "-preset",
      "medium",
      "-crf",
    ])
    .arg(crf.to_string())
    // Joining distant screen moments can look like a scene cut that does not
    // exist in the real timeline. Do not let those synthetic seams introduce
    // extra I-frames and recreate the overestimate this path avoids.
    .args([
      "-sc_threshold",
      "0",
      "-pix_fmt",
      "yuv420p",
      "-profile:v",
      "high",
      "-f",
      "h264",
    ])
    .arg(&temporary);
  let output = command.output().map_err(|error| {
    let _ = std::fs::remove_file(&temporary);
    format!("FFmpeg could not start the size estimate: {error}")
  })?;

  if !output.status.success() {
    let _ = std::fs::remove_file(&temporary);
    let detail = String::from_utf8_lossy(&output.stderr);
    let detail = detail.trim();
    return Err(if detail.is_empty() {
      "FFmpeg could not estimate the compressed video".to_owned()
    } else {
      format!("FFmpeg could not estimate the compressed video: {detail}")
    });
  }
  let metadata = std::fs::metadata(&temporary);
  let _ = std::fs::remove_file(&temporary);
  let sample_bytes = metadata.map_err(|error| error.to_string())?.len();
  let sampled_seconds = sample_duration * sample_count as f64;

  Ok(((sample_bytes as f64 / sampled_seconds) * duration).round() as u64)
}

/// The preview files built so far for the artifact on screen.
///
/// Kept rather than rebuilt because toggling a track off and back on is the
/// most ordinary thing a person does here, and the file for that combination
/// is already on disk. They belong to one artifact at a time: a replacement
/// makes every one of them garbage.
#[derive(Default)]
pub struct PreviewMixes {
  artifact_id: u64,
  by_signature: HashMap<String, PathBuf>,
}

impl PreviewMixes {
  pub fn cleanup(&mut self) {
    for (_, path) in self.by_signature.drain() {
      let _ = std::fs::remove_file(path);
    }
    self.artifact_id = 0;
  }

  /// The file for this combination, if it was built and is still worth
  /// playing.
  ///
  /// A remembered name is not a promise: the file behind it can have been
  /// swept, or left empty by an earlier run that died mid-encode. Serving one
  /// of those is worse than rebuilding, because the window plays it once and
  /// then holds a broken preview for as long as the artifact is open, so a
  /// name that no longer stands up is forgotten and the mix is made again.
  /// Only the cheap checks belong here - a probe on every request would cost a
  /// process launch each time a track is toggled.
  fn cached(&mut self, artifact_id: u64, signature: &str) -> Option<PathBuf> {
    if self.artifact_id != artifact_id {
      return None;
    }

    let path = self.by_signature.get(signature)?.clone();
    if holds_bytes(&path) {
      return Some(path);
    }

    self.by_signature.remove(signature);
    let _ = std::fs::remove_file(&path);

    None
  }

  fn remember(&mut self, artifact_id: u64, signature: String, path: PathBuf) {
    if self.artifact_id != artifact_id {
      self.cleanup();
      self.artifact_id = artifact_id;
    }
    if let Some(replaced) = self.by_signature.insert(signature, path) {
      // Only reachable if the same combination was built twice, in which case
      // the two names are equal and there is nothing to remove.
      let _ = std::fs::remove_file(replaced);
    }
  }
}

/// Whether the recording can be played as it is, or only through a mix.
///
/// A media element renders the *first* audio track of a file and nothing else.
/// WebKit does not even list the others: a two-track recording opened in a
/// WKWebView reports `audioTracks.length` of one, so no amount of enabling
/// tracks from script reaches the second. Measured against a recording whose
/// system-audio track was written first and captured silence, the element
/// played it at `readyState` 4 from end to end, fired `playing` at once, and
/// made no sound whatsoever - the microphone underneath it was simply never
/// rendered.
///
/// So a recording carrying more than one audio track is not a stand-in for its
/// own mix. With one track there is nothing to sum, and stream-copying a
/// multi-gigabyte movie to arrive back at the file we started from would be
/// seconds of work for no difference.
pub const fn plays_without_mixing(tracks: &[RecordingAudioTrack]) -> bool {
  tracks.len() <= 1
}

/// The file the window should play for a given set of enabled tracks.
///
/// Returns the recording itself when no mixing could change what is heard - see
/// [`plays_without_mixing`] for exactly when that is.
pub fn preview_mix(
  mixes: &mut PreviewMixes,
  artifact_id: u64,
  source: &Path,
  tracks: &[RecordingAudioTrack],
  selection: &TrackSelection,
) -> Result<PathBuf, String> {
  if plays_without_mixing(tracks) && selection.covers(tracks) {
    return Ok(source.to_owned());
  }

  let signature = selection.signature();
  if let Some(cached) = mixes.cached(artifact_id, &signature) {
    return Ok(cached);
  }

  let destination = mix_path(source, artifact_id, &signature);
  mix(source, &destination, selection)?;
  mixes.remember(artifact_id, signature, destination.clone());

  Ok(destination)
}

#[cfg(test)]
mod tests {
  use super::*;

  /// A directory of this module's own, so a test that writes files cannot be
  /// confused by anything else on the machine.
  fn test_directory(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join("orbit-capture-tests").join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
  }

  #[test]
  fn recognises_its_own_derivatives_by_name() {
    assert!(is_preview_file(Path::new("/tmp/preview-42-7-mix-0-1.mp4")));
    // An abandoned encode too: it is named after the mix it was going to
    // become, so the startup sweep reclaims it without knowing it exists.
    assert!(is_preview_file(Path::new(
      "/tmp/preview-42-7-mix-0-1.mp4.3.part"
    )));
    assert!(!is_preview_file(Path::new(
      "/tmp/recording-20260808-143205.000.mp4"
    )));
  }

  #[test]
  fn names_a_mix_after_the_combination_it_holds() {
    let source = Path::new("/tmp/recording-123.mp4");
    assert_eq!(
      mix_path_for(source, 42, 7, "0-1"),
      Path::new("/tmp/preview-42-7-mix-0-1.mp4")
    );
  }

  #[test]
  fn keeps_two_instances_off_one_another_s_mixes() {
    let source = Path::new("/tmp/recording-123.mp4");
    // Both processes are on their first artifact and the same combination of
    // tracks; only the process id keeps them apart.
    assert_ne!(
      mix_path_for(source, 42, 1, "0-1"),
      mix_path_for(source, 43, 1, "0-1")
    );
  }

  #[test]
  fn encodes_beside_the_mix_rather_than_onto_it() {
    let destination = mix_path_for(Path::new("/tmp/recording-123.mp4"), 42, 7, "0-1");
    let temporary = mix_temp_path(&destination);

    assert_ne!(temporary, destination);
    assert_eq!(temporary.parent(), destination.parent());
    assert!(is_preview_file(&temporary));
    assert_eq!(temporary.extension().unwrap(), "part");
    // Two encodes in flight at once still write to files of their own.
    assert_ne!(mix_temp_path(&destination), temporary);
  }

  #[test]
  fn plays_a_remembered_mix_only_while_it_stands_up() {
    let directory = test_directory("preview-mix-cache");
    let healthy = directory.join("preview-42-7-mix-0.mp4");
    let empty = directory.join("preview-42-7-mix-1.mp4");
    let missing = directory.join("preview-42-7-mix-2.mp4");
    std::fs::write(&healthy, b"not really a movie, but not nothing").unwrap();
    std::fs::write(&empty, b"").unwrap();

    let mut mixes = PreviewMixes::default();
    mixes.remember(7, "0".to_owned(), healthy.clone());
    mixes.remember(7, "1".to_owned(), empty.clone());
    mixes.remember(7, "2".to_owned(), missing.clone());

    assert_eq!(mixes.cached(7, "0"), Some(healthy));
    // A truncation left by an interrupted encode is not a preview, and neither
    // is a name whose file has been swept from under it.
    assert_eq!(mixes.cached(7, "1"), None);
    assert_eq!(mixes.cached(7, "2"), None);
    // Forgotten as well as refused, so the next request rebuilds them.
    assert!(!mixes.by_signature.contains_key("1"));
    assert!(!mixes.by_signature.contains_key("2"));
    assert!(!empty.exists());
  }

  fn tracks(count: usize) -> Vec<RecordingAudioTrack> {
    (0..count)
      .map(|stream_index| RecordingAudioTrack {
        kind: AudioTrackKind::Unknown,
        label: format!("Audio {}", stream_index + 1),
        stream_index,
      })
      .collect()
  }

  #[test]
  fn lets_a_recording_stand_in_for_itself_only_while_one_track_can_be_heard() {
    assert!(plays_without_mixing(&tracks(0)));
    assert!(plays_without_mixing(&tracks(1)));
    // The second track would be inaudible in a media element, so the mix is
    // the only thing that carries what was recorded.
    assert!(!plays_without_mixing(&tracks(2)));
    assert!(!plays_without_mixing(&tracks(3)));
  }

  #[test]
  fn writes_one_timeline_the_picture_is_copied_onto() {
    let both = tracks(2);
    let selection = TrackSelection::new(&both, &[0, 1]);

    assert_eq!(
      mix_args(
        Path::new("/tmp/recording-123.mp4"),
        Path::new("/tmp/out.mp4"),
        &selection
      ),
      [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-y",
        "-i",
        "/tmp/recording-123.mp4",
        "-map",
        "0:v:0",
        "-c:v",
        "copy",
        "-filter_complex",
        "[0:a:0][0:a:1]amix=inputs=2:normalize=0[mix]",
        "-map",
        "[mix]",
        "-c:a",
        "aac",
        "-b:a",
        "192k",
        // Named rather than guessed: the file this is written to is a `.part`,
        // and FFmpeg will not start without being told what to make of it.
        "-f",
        "mp4",
        "-movflags",
        // Signed composition offsets keep FFmpeg from inserting an edit that
        // starts after the H.264 decoder's initial reference frames. WebKit is
        // far less forgiving of that edit than ordinary movie players.
        "+faststart+negative_cts_offsets",
        "/tmp/out.mp4",
      ]
      .map(OsString::from)
    );
  }

  #[test]
  fn copies_every_stream_of_the_recording_into_the_saved_movie() {
    assert_eq!(
      remux_args(
        Path::new("/tmp/recording-123.mov"),
        Path::new("/tmp/Keeper.mp4")
      ),
      [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-y",
        "-i",
        "/tmp/recording-123.mov",
        // `-map 0` rather than one stream of each kind: a recording can carry
        // system audio *and* a microphone, and losing one of them here would
        // be silent. `-c copy` because the encode already happened.
        "-map",
        "0",
        "-c",
        "copy",
        "-f",
        "mp4",
        "-movflags",
        "+faststart",
        "/tmp/Keeper.mp4",
      ]
      .map(OsString::from)
    );
  }

  #[test]
  fn maps_only_the_selected_tracks_when_the_saved_audio_changes() {
    let available = tracks(3);
    let selection = TrackSelection::new(&available, &[0, 2]);

    assert_eq!(
      selected_export_args(
        Path::new("/tmp/recording-123.mov"),
        Path::new("/tmp/Keeper.mp4"),
        &selection,
        AudioLayout::SeparateTracks,
        VideoExportOptions {
          compression: 0,
          resolution_scale_percent: 200,
          source_scale_percent: 200,
        },
      ),
      [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-y",
        "-i",
        "/tmp/recording-123.mov",
        "-progress",
        "pipe:1",
        "-nostats",
        "-map",
        "0:v:0?",
        "-c:v",
        "copy",
        "-map",
        "0:a:0",
        "-map",
        "0:a:2",
        "-c:a",
        "copy",
        "-f",
        "mp4",
        "-movflags",
        "+faststart",
        "/tmp/Keeper.mp4",
      ]
      .map(OsString::from)
    );
  }

  #[test]
  fn maps_compression_to_a_cross_platform_quality_encode() {
    let available = tracks(1);
    let selection = TrackSelection::new(&available, &[0]);

    let args = selected_export_args(
      Path::new("/tmp/recording.mov"),
      Path::new("/tmp/Keeper.mp4"),
      &selection,
      AudioLayout::SeparateTracks,
      VideoExportOptions {
        compression: 2,
        resolution_scale_percent: 200,
        source_scale_percent: 200,
      },
    );
    let args = args
      .iter()
      .map(|argument| argument.to_string_lossy())
      .collect::<Vec<_>>();

    assert!(args.windows(2).any(|pair| pair == ["-c:v", "libx264"]));
    assert!(args.windows(2).any(|pair| pair == ["-crf", "24"]));
    assert!(args.windows(2).any(|pair| pair == ["-preset", "medium"]));
  }

  #[test]
  fn maps_the_compression_edges_without_reencoding_original() {
    assert_eq!(compression_crf(0), None);
    assert_eq!(compression_crf(1), Some(20));
    assert_eq!(compression_crf(2), Some(24));
    assert_eq!(compression_crf(3), Some(28));
    assert_eq!(compression_crf(4), Some(32));
    assert_eq!(compression_crf(255), Some(32));
  }

  #[test]
  fn downsampling_uses_lanczos_and_requires_an_encode() {
    assert_eq!(resolution_filter(200, 200), None);
    assert_eq!(
      resolution_filter(200, 100).as_deref(),
      Some("scale=trunc(iw*100/200/2)*2:trunc(ih*100/200/2)*2:flags=lanczos")
    );
    assert_eq!(export_crf(0, true), Some(20));
  }

  #[test]
  fn estimate_joins_seeked_samples_before_one_encode() {
    assert_eq!(
      estimate_filter(3, Some("scale=iw/2:ih/2")),
      "[0:v:0]setpts=PTS-STARTPTS[sample0];[1:v:0]setpts=PTS-STARTPTS[sample1];[2:v:0]setpts=PTS-STARTPTS[sample2];[sample0][sample1][sample2]concat=n=3:v=1:a=0,scale=iw/2:ih/2[estimated]"
    );
    assert_eq!(
      estimate_filter(1, None),
      "[0:v:0]setpts=PTS-STARTPTS[estimated]"
    );
  }

  #[test]
  fn reads_ffmpeg_progress_as_milliseconds() {
    assert_eq!(progress_milliseconds("out_time_us=1234567"), Some(1_234));
    assert_eq!(progress_milliseconds("progress=continue"), None);
    assert_eq!(progress_milliseconds("out_time_us=N/A"), None);
  }

  #[test]
  fn estimates_and_compresses_a_real_movie_when_x264_is_available() {
    if !supports_compression() {
      eprintln!("skipped: this FFmpeg does not include libx264");
      return;
    }

    let directory = test_directory("compressed-export");
    let source = directory.join("source.mov");
    let destination = directory.join("compressed.mp4");
    let built = Command::new(ffmpeg_path())
      .args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=320x240:rate=30:duration=3",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=3",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=880:duration=3",
        "-map",
        "0:v",
        "-map",
        "1:a",
        "-map",
        "2:a",
        "-c:v",
        "libx264",
        "-crf",
        "18",
        "-c:a",
        "aac",
      ])
      .arg(&source)
      .status();
    if !built.is_ok_and(|status| status.success()) {
      eprintln!("skipped: FFmpeg could not build the test movie");
      return;
    }

    let available = tracks(2);
    let selection = TrackSelection::new(&available, &[0, 1]);
    let estimated = estimate_compressed_video_bytes(&source, 3_000, 2, 200, 100).unwrap();
    assert!(estimated > 0);
    let cancelled = AtomicBool::new(false);
    let mut progress = Vec::new();
    export_selected_recording(
      &source,
      &destination,
      &selection,
      AudioLayout::Mixdown,
      ExportRunOptions {
        cancelled: &cancelled,
        on_progress: &mut |milliseconds| progress.push(milliseconds),
        video: VideoExportOptions {
          compression: 2,
          resolution_scale_percent: 100,
          source_scale_percent: 200,
        },
      },
    )
    .unwrap();

    assert!(holds_bytes(&source));
    assert!(plays_from_start_to_end(&destination));
    assert!(progress
      .last()
      .is_some_and(|milliseconds| *milliseconds > 0));
    let output = Command::new(ffmpeg_path())
      .args(["-hide_banner", "-i"])
      .arg(&destination)
      .output()
      .unwrap();
    assert!(String::from_utf8_lossy(&output.stderr).contains("160x120"));
    let estimated_audio = selection.estimated_audio_bytes(&available, AudioLayout::Mixdown, 3_000);
    let estimated_media = estimated.saturating_add(estimated_audio);
    let predicted = estimated_media
      .saturating_add(estimated_media / 200)
      .saturating_add(4_096);
    let actual = std::fs::metadata(&destination).unwrap().len();
    assert!(predicted.abs_diff(actual) <= actual * 2 / 5);
    let described = Command::new(ffmpeg_path())
      .args(["-hide_banner", "-nostdin", "-i"])
      .arg(&destination)
      .output()
      .unwrap();
    let streams = String::from_utf8_lossy(&described.stderr)
      .lines()
      .filter(|line| line.trim_start().starts_with("Stream #"))
      .count();
    // One re-encoded video stream and the two selected audio streams mixed to
    // one, which verifies both choices are applied by the same output pass.
    assert_eq!(streams, 2);

    let cancelled_destination = directory.join("cancelled.mp4");
    let cancelled = AtomicBool::new(true);
    let result = export_selected_recording(
      &source,
      &cancelled_destination,
      &selection,
      AudioLayout::Mixdown,
      ExportRunOptions {
        cancelled: &cancelled,
        on_progress: &mut |_| {},
        video: VideoExportOptions {
          compression: 2,
          resolution_scale_percent: 100,
          source_scale_percent: 200,
        },
      },
    )
    .unwrap();
    assert_eq!(result, ExportRunResult::Cancelled);
    assert!(holds_bytes(&source));
    assert!(!cancelled_destination.exists());
  }

  #[test]
  fn copies_beside_the_saved_movie_rather_than_onto_it() {
    let destination = Path::new("/tmp/Keeper.mp4");
    let temporary = remux_temp_path(destination);

    assert_ne!(temporary, destination);
    // A sibling, so the rename that publishes it cannot cross a volume - the
    // destination is wherever the user chose, which is often an external disk.
    assert_eq!(temporary.parent(), destination.parent());
    assert_eq!(temporary.extension().unwrap(), "part");
    // Not something the user has to look at while it is being written.
    assert!(temporary
      .file_name()
      .unwrap()
      .to_string_lossy()
      .starts_with('.'));
    // Two saves at once still write to files of their own.
    assert_ne!(remux_temp_path(destination), temporary);
  }
}
