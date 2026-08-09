// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

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

pub(super) fn tool_path(name: &str) -> PathBuf {
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

pub(super) fn ffmpeg_path() -> PathBuf {
  tool_path("ffmpeg")
}

/// FFprobe is only ever used to *check* a file this module just wrote, never
/// to produce one. Everything here still works without it - see
/// [`plays_from_start_to_end`] for what is given up when it is missing.
pub(super) fn ffprobe_path() -> PathBuf {
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
