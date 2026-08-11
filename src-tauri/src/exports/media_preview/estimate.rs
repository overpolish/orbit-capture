// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// The H.264 quality represented by the compression control.
///
/// Zero is deliberately not a CRF: it means the original encoded video is
/// copied without a generation loss. The remaining values are named quality
/// steps in the UI, so each maps to a stable encoder setting.
pub(super) fn compression_crf(compression: u8) -> Option<u16> {
  match compression {
    0 => None,
    1 => Some(20),
    2 => Some(24),
    3 => Some(28),
    _ => Some(32),
  }
}

pub(super) fn resolution_filter(
  source_scale_percent: u16,
  resolution_scale_percent: u16,
) -> Option<String> {
  (resolution_scale_percent < source_scale_percent).then(|| {
    format!(
      "scale=trunc(iw*{resolution_scale_percent}/{source_scale_percent}/2)*2:trunc(ih*{resolution_scale_percent}/{source_scale_percent}/2)*2:flags=lanczos"
    )
  })
}

/// Resizing cannot stream-copy. High quality is deliberately used if a caller
/// requests a smaller resolution with Original compression.
pub(in crate::exports) fn export_crf(compression: u8, is_resizing: bool) -> Option<u16> {
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

pub(super) fn estimate_temp_path(source: &Path) -> PathBuf {
  let attempt = ESTIMATE_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
  source.with_file_name(format!(
    "{PREVIEW_PREFIX}estimate-{}-{attempt}.h264.part",
    std::process::id()
  ))
}

pub(super) fn estimate_filter(sample_count: usize, scale_filter: Option<&str>) -> String {
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
