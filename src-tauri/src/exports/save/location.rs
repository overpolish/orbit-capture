// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{path::Path, process::Command};

pub(super) fn open_containing_folder(path: &Path) -> Result<(), String> {
  let directory = path
    .parent()
    .ok_or_else(|| "The exported file has no containing folder".to_owned())?;
  #[cfg(target_os = "macos")]
  let opened = Command::new("open").arg(directory).spawn();
  #[cfg(target_os = "windows")]
  let opened = Command::new("explorer").arg(directory).spawn();
  #[cfg(not(any(target_os = "macos", target_os = "windows")))]
  let opened = Command::new("xdg-open").arg(directory).spawn();
  opened.map(|_| ()).map_err(|error| error.to_string())
}
