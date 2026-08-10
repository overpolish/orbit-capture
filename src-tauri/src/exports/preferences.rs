// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[derive(Default, Deserialize, Serialize)]
struct ExportPreferences {
  screenshot_radius_percent: f64,
}

fn preferences_path(app: &AppHandle) -> Result<PathBuf, String> {
  app
    .path()
    .app_config_dir()
    .map(|directory| directory.join(EXPORT_PREFERENCES_FILE))
    .map_err(|error| error.to_string())
}

pub(super) fn load_screenshot_radius(app: &AppHandle) -> f64 {
  preferences_path(app)
    .ok()
    .and_then(|path| std::fs::read(path).ok())
    .and_then(|contents| serde_json::from_slice::<ExportPreferences>(&contents).ok())
    .map_or(0.0, |preferences| {
      validate_screenshot_radius(preferences.screenshot_radius_percent).unwrap_or(0.0)
    })
}

pub(super) fn store_screenshot_radius(app: &AppHandle, radius: f64) -> Result<(), String> {
  let path = preferences_path(app)?;
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
  }
  let contents = serde_json::to_vec_pretty(&ExportPreferences {
    screenshot_radius_percent: radius,
  })
  .map_err(|error| error.to_string())?;
  std::fs::write(path, contents).map_err(|error| error.to_string())
}

pub(super) fn validate_screenshot_radius(radius: f64) -> Result<f64, String> {
  if !radius.is_finite() || !(0.0..=50.0).contains(&radius) {
    return Err("The screenshot corner radius is not valid".to_owned());
  }
  Ok(radius)
}

pub(super) fn remember_screenshot_radius(app: &AppHandle, radius: f64) -> Result<f64, String> {
  let radius = validate_screenshot_radius(radius)?;
  *app
    .state::<ExportState>()
    .screenshot_radius_percent
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner()) = radius;
  store_screenshot_radius(app, radius)?;
  Ok(radius)
}
