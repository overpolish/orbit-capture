// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

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
