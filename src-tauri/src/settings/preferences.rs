// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{path::PathBuf, sync::RwLock};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;

const SETTINGS_FILE: &str = "settings.json";
const SETTINGS_CHANGED_EVENT: &str = "settings://changed";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GeneralSettings {
  pub recording_directory: Option<PathBuf>,
  pub screenshot_directory: Option<PathBuf>,
  pub open_location_after_export: bool,
  pub show_recording_confidence_checks: bool,
  pub launch_at_login: bool,
  pub show_recording_bar_on_launch: bool,
  pub recording_countdown_seconds: u8,
}

impl Default for GeneralSettings {
  fn default() -> Self {
    Self {
      recording_directory: None,
      screenshot_directory: None,
      open_location_after_export: false,
      show_recording_confidence_checks: true,
      launch_at_login: false,
      show_recording_bar_on_launch: true,
      recording_countdown_seconds: 0,
    }
  }
}

#[derive(Default)]
pub struct GeneralSettingsState(RwLock<GeneralSettings>);

fn path(app: &AppHandle) -> Result<PathBuf, String> {
  app
    .path()
    .app_config_dir()
    .map(|directory| directory.join(SETTINGS_FILE))
    .map_err(|error| error.to_string())
}

fn read(app: &AppHandle) -> GeneralSettings {
  path(app)
    .ok()
    .and_then(|path| std::fs::read(path).ok())
    .and_then(|contents| serde_json::from_slice(&contents).ok())
    .unwrap_or_default()
}

fn write(app: &AppHandle, settings: &GeneralSettings) -> Result<(), String> {
  let path = path(app)?;
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
  }
  let contents = serde_json::to_vec_pretty(settings).map_err(|error| error.to_string())?;
  std::fs::write(path, contents).map_err(|error| error.to_string())
}

fn validate(settings: &GeneralSettings) -> Result<(), String> {
  if !matches!(settings.recording_countdown_seconds, 0 | 3 | 5) {
    return Err("The countdown must be off, 3 seconds or 5 seconds".to_owned());
  }
  for directory in [
    settings.recording_directory.as_ref(),
    settings.screenshot_directory.as_ref(),
  ]
  .into_iter()
  .flatten()
  {
    if !directory.is_dir() {
      return Err(format!("{} is no longer available", directory.display()));
    }
  }
  Ok(())
}

pub fn initialize(app: &AppHandle) {
  let mut settings = read(app);
  settings.launch_at_login = app.autolaunch().is_enabled().unwrap_or(false);
  *app
    .state::<GeneralSettingsState>()
    .0
    .write()
    .unwrap_or_else(|poisoned| poisoned.into_inner()) = settings;
}

pub fn current(app: &AppHandle) -> GeneralSettings {
  app
    .state::<GeneralSettingsState>()
    .0
    .read()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .clone()
}

#[tauri::command]
pub fn get_general_settings(app: AppHandle) -> GeneralSettings {
  current(&app)
}

#[tauri::command]
pub fn set_general_settings(
  app: AppHandle,
  mut settings: GeneralSettings,
) -> Result<GeneralSettings, String> {
  validate(&settings)?;
  let autolaunch = app.autolaunch();
  if settings.launch_at_login {
    autolaunch.enable().map_err(|error| error.to_string())?;
  } else {
    autolaunch.disable().map_err(|error| error.to_string())?;
  }
  settings.launch_at_login = autolaunch.is_enabled().unwrap_or(settings.launch_at_login);
  write(&app, &settings)?;
  *app
    .state::<GeneralSettingsState>()
    .0
    .write()
    .unwrap_or_else(|poisoned| poisoned.into_inner()) = settings.clone();
  let _ = app.emit(SETTINGS_CHANGED_EVENT, &settings);
  Ok(settings)
}

#[tauri::command]
pub async fn browse_default_location(
  app: AppHandle,
  kind: String,
) -> Result<Option<PathBuf>, String> {
  let settings = current(&app);
  let start = match kind.as_str() {
    "recording" => settings.recording_directory,
    "screenshot" => settings.screenshot_directory,
    _ => return Err("Unknown default location".to_owned()),
  };
  let parent = app.get_webview_window(crate::windows::WindowLabel::Settings.as_str());
  tauri::async_runtime::spawn_blocking(move || {
    use tauri_plugin_dialog::DialogExt;
    let mut dialog = app.dialog().file().set_title("Choose a folder");
    if let Some(start) = start {
      dialog = dialog.set_directory(start);
    }
    if let Some(parent) = parent {
      dialog = dialog.set_parent(&parent);
    }
    dialog
      .blocking_pick_folder()
      .and_then(|path| path.into_path().ok())
  })
  .await
  .map_err(|error| error.to_string())
}
