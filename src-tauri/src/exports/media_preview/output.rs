// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// The established MP4 container settings used by saved recordings.
pub(super) const EXPORT_MP4_OUTPUT: [&str; 4] = ["-f", "mp4", "-movflags", "+faststart"];

/// How much of FFmpeg's final error is useful to surface to the window.
pub(super) const OUTPUT_ERROR_DETAIL: usize = 400;

/// Whether a finished encode is a file a player can actually open.
pub(super) fn plays_from_start_to_end(path: &Path) -> bool {
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

pub(super) fn holds_bytes(path: &Path) -> bool {
  std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
}
