// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

pub(super) fn remux_args(source: &Path, destination: &Path) -> Vec<OsString> {
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
pub(super) fn selected_export_args(
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
pub(super) fn remux_temp_path(destination: &Path) -> PathBuf {
  let attempt = REMUX_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
  let name = destination.file_name().map_or_else(
    || "recording".to_owned(),
    |name| name.to_string_lossy().into_owned(),
  );

  destination.with_file_name(format!(".{name}.{}.{attempt}.part", std::process::id()))
}

pub(super) fn remux_error(stderr: &[u8]) -> String {
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
