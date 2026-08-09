// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

pub(super) fn directory_path(app: &AppHandle) -> tauri::Result<PathBuf> {
  Ok(app.path().app_config_dir()?.join(EXPORT_DIRECTORY_FILE))
}

pub(super) fn load_directory(app: &AppHandle) -> Option<PathBuf> {
  let stored = directory_path(app)
    .ok()
    .and_then(|path| std::fs::read(path).ok())
    .and_then(|contents| serde_json::from_slice::<PathBuf>(&contents).ok())?;

  stored.is_dir().then_some(stored)
}

pub(super) fn store_directory(app: &AppHandle, directory: &Path) -> tauri::Result<()> {
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
pub(super) fn current_directory(app: &AppHandle) -> Option<PathBuf> {
  let state = app.state::<ExportState>();
  let remembered = state
    .directory
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .clone();

  remembered.or_else(|| screenshot_directory(app).ok())
}
