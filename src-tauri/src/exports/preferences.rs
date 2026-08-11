// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[derive(Deserialize, Serialize)]
#[serde(default)]
struct ExportPreferences {
  cursor_effects: cursor_effects::CursorEffectSettings,
  open_location_after_export: bool,
  screenshot_radius_percent: f64,
}

impl Default for ExportPreferences {
  fn default() -> Self {
    Self {
      cursor_effects: cursor_effects::CursorEffectSettings::default(),
      open_location_after_export: false,
      screenshot_radius_percent: 0.0,
    }
  }
}

fn preferences_path(app: &AppHandle) -> Result<PathBuf, String> {
  app
    .path()
    .app_config_dir()
    .map(|directory| directory.join(EXPORT_PREFERENCES_FILE))
    .map_err(|error| error.to_string())
}

pub(super) fn load_screenshot_radius(app: &AppHandle) -> f64 {
  load_preferences(app).map_or(0.0, |preferences| {
    validate_screenshot_radius(preferences.screenshot_radius_percent).unwrap_or(0.0)
  })
}

pub(super) fn load_cursor_effects(app: &AppHandle) -> cursor_effects::CursorEffectSettings {
  load_preferences(app)
    .and_then(|preferences| validate_cursor_effects(preferences.cursor_effects).ok())
    .unwrap_or_default()
}

pub(super) fn load_open_location_after_export(app: &AppHandle) -> bool {
  load_preferences(app).is_some_and(|preferences| preferences.open_location_after_export)
}

fn load_preferences(app: &AppHandle) -> Option<ExportPreferences> {
  preferences_path(app)
    .ok()
    .and_then(|path| std::fs::read(path).ok())
    .and_then(|contents| serde_json::from_slice::<ExportPreferences>(&contents).ok())
}

fn store_preferences(app: &AppHandle, preferences: &ExportPreferences) -> Result<(), String> {
  let path = preferences_path(app)?;
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
  }
  let contents = serde_json::to_vec_pretty(preferences).map_err(|error| error.to_string())?;
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
  let mut preferences = load_preferences(app).unwrap_or_default();
  preferences.screenshot_radius_percent = radius;
  store_preferences(app, &preferences)?;
  Ok(radius)
}

fn validate_cursor_effects(
  effects: cursor_effects::CursorEffectSettings,
) -> Result<cursor_effects::CursorEffectSettings, String> {
  if !effects.size_percent.is_finite() || !(50.0..=500.0).contains(&effects.size_percent) {
    return Err("The cursor size is not valid".to_owned());
  }
  Ok(effects)
}

pub(super) fn remember_cursor_effects(
  app: &AppHandle,
  effects: cursor_effects::CursorEffectSettings,
) -> Result<(), String> {
  let effects = validate_cursor_effects(effects)?;
  *app
    .state::<ExportState>()
    .cursor_effects
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner()) = effects;
  let mut preferences = load_preferences(app).unwrap_or_default();
  preferences.cursor_effects = effects;
  store_preferences(app, &preferences)
}

pub(super) fn remember_open_location_after_export(
  app: &AppHandle,
  open_location_after_export: bool,
) -> Result<(), String> {
  *app
    .state::<ExportState>()
    .open_location_after_export
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner()) = open_location_after_export;
  let mut preferences = load_preferences(app).unwrap_or_default();
  preferences.open_location_after_export = open_location_after_export;
  store_preferences(app, &preferences)
}
