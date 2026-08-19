// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{path::PathBuf, sync::Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::windows::{self, WindowLabel};

const SHORTCUTS_FILE: &str = "shortcuts.json";
const SHORTCUT_ACTION_EVENT: &str = "global-shortcut://action";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ShortcutAction {
  ToggleRecordingBar,
  StartStopRecording,
  PauseResumeRecording,
  TakeScreenshot,
  RecognizeText,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutBinding {
  pub action: ShortcutAction,
  pub shortcut: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutSettings {
  pub bindings: Vec<ShortcutBinding>,
}

impl Default for ShortcutSettings {
  fn default() -> Self {
    Self {
      bindings: vec![
        ShortcutBinding {
          action: ShortcutAction::ToggleRecordingBar,
          shortcut: Some("CommandOrControl+Shift+Digit6".to_owned()),
        },
        ShortcutBinding {
          action: ShortcutAction::StartStopRecording,
          shortcut: None,
        },
        ShortcutBinding {
          action: ShortcutAction::PauseResumeRecording,
          shortcut: None,
        },
        ShortcutBinding {
          action: ShortcutAction::TakeScreenshot,
          shortcut: Some("CommandOrControl+Shift+Digit8".to_owned()),
        },
        ShortcutBinding {
          action: ShortcutAction::RecognizeText,
          shortcut: Some("CommandOrControl+Shift+KeyT".to_owned()),
        },
      ],
    }
  }
}

#[derive(Default)]
pub struct ShortcutSettingsState(Mutex<ShortcutSettings>);

fn settings_path(app: &AppHandle) -> tauri::Result<PathBuf> {
  Ok(app.path().app_config_dir()?.join(SHORTCUTS_FILE))
}

fn load(app: &AppHandle) -> ShortcutSettings {
  let stored = settings_path(app)
    .ok()
    .and_then(|path| std::fs::read(path).ok())
    .and_then(|contents| serde_json::from_slice::<ShortcutSettings>(&contents).ok());
  let mut settings = ShortcutSettings::default();
  if let Some(stored) = stored {
    for binding in &mut settings.bindings {
      binding.shortcut = stored
        .bindings
        .iter()
        .find(|candidate| candidate.action == binding.action)
        .and_then(|candidate| candidate.shortcut.clone());
    }
  }
  settings
}

fn store(app: &AppHandle, settings: &ShortcutSettings) -> Result<(), String> {
  let path = settings_path(app).map_err(|error| error.to_string())?;
  if let Some(directory) = path.parent() {
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
  }
  let contents = serde_json::to_vec_pretty(settings).map_err(|error| error.to_string())?;
  std::fs::write(path, contents).map_err(|error| error.to_string())
}

/// The window that carries out an action the frontend owns.
///
/// The label states ownership; it does not scope delivery. The frontend's
/// `listen` registers for any target, so every window receives every one of
/// these events and each listener has to match on the action itself.
const fn action_window(action: ShortcutAction) -> Option<WindowLabel> {
  match action {
    ShortcutAction::StartStopRecording => Some(WindowLabel::RecordingBar),
    // The overlay opens itself in region-edit mode and captures what the user
    // settles on, so the screenshot never goes near the recording bar.
    ShortcutAction::TakeScreenshot => Some(WindowLabel::RegionSelector),
    ShortcutAction::ToggleRecordingBar
    | ShortcutAction::PauseResumeRecording
    | ShortcutAction::RecognizeText => None,
  }
}

fn notify_frontend(app: &AppHandle, action: ShortcutAction) {
  if let Some(window) = action_window(action) {
    let _ = app.emit_to(window.as_str(), SHORTCUT_ACTION_EVENT, action);
  }
}

fn run_action(app: &AppHandle, action: ShortcutAction) {
  if action != ShortcutAction::RecognizeText {
    crate::text_recognition::dismiss(app);
  }
  match action {
    ShortcutAction::ToggleRecordingBar => {
      if crate::recording::is_idle(app) {
        if crate::exports::focus_pending_workspace(app) {
          return;
        }
        // Tauri's visibility query describes the original webview window and
        // is not authoritative after macOS converts it into an NSPanel.
        if windows::is_recording_ui_visible() {
          let _ = windows::hide_recording_ui(app.clone());
        } else {
          #[cfg(target_os = "macos")]
          if !crate::permissions::has_required_recording_permissions(app) {
            let _ = crate::permissions::show_permissions_window(app);
            return;
          }
          let _ = windows::show_recording_ui(app);
        }
      }
    }
    ShortcutAction::PauseResumeRecording => {
      if matches!(
        crate::recording::snapshot(app).status,
        crate::recording::RecordingStatus::Recording | crate::recording::RecordingStatus::Paused
      ) {
        let _ = crate::recording::toggle_pause(app);
      }
    }
    ShortcutAction::StartStopRecording => match crate::recording::snapshot(app).status {
      crate::recording::RecordingStatus::Idle => {
        if !crate::exports::focus_pending_workspace(app) {
          notify_frontend(app, action);
        }
      }
      crate::recording::RecordingStatus::Recording | crate::recording::RecordingStatus::Paused => {
        let _ = crate::recording::stop(app);
      }
      crate::recording::RecordingStatus::Starting => {
        let _ = crate::recording::cancel(app);
      }
      crate::recording::RecordingStatus::Stopping => {}
    },
    ShortcutAction::TakeScreenshot => {
      if crate::recording::is_idle(app)
        && !crate::exports::focus_if_screenshot_workspace_blocked(app)
      {
        notify_frontend(app, action);
      }
    }
    ShortcutAction::RecognizeText => {
      if let Err(error) = crate::text_recognition::start(app) {
        eprintln!("Could not start text recognition: {error}");
      }
    }
  }
}

pub fn shortcut_for(app: &AppHandle, action: ShortcutAction) -> Option<String> {
  app
    .state::<ShortcutSettingsState>()
    .0
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .bindings
    .iter()
    .find(|binding| binding.action == action)
    .and_then(|binding| binding.shortcut.clone())
}

fn register_binding(app: &AppHandle, action: ShortcutAction, shortcut: &str) -> Result<(), String> {
  let parsed = shortcut
    .parse::<Shortcut>()
    .map_err(|error| error.to_string())?;
  app
    .global_shortcut()
    .on_shortcut(parsed, move |app, _, event| {
      if event.state() == ShortcutState::Pressed {
        run_action(app, action);
      }
    })
    .map_err(|error| error.to_string())
}

pub fn initialize(app: &AppHandle) {
  let settings = load(app);
  for binding in &settings.bindings {
    if let Some(shortcut) = binding.shortcut.as_deref() {
      if let Err(error) = register_binding(app, binding.action, shortcut) {
        eprintln!("Could not register {shortcut}: {error}");
      }
    }
  }
  *app
    .state::<ShortcutSettingsState>()
    .0
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner()) = settings;
  crate::tray::refresh(app);
}

#[tauri::command]
pub fn begin_shortcut_capture(app: AppHandle) -> Result<(), String> {
  app
    .global_shortcut()
    .unregister_all()
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn end_shortcut_capture(app: AppHandle) -> Result<(), String> {
  let settings = app
    .state::<ShortcutSettingsState>()
    .0
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .clone();
  for binding in settings.bindings {
    if let Some(shortcut) = binding.shortcut {
      let parsed = shortcut
        .parse::<Shortcut>()
        .map_err(|error| error.to_string())?;
      if !app.global_shortcut().is_registered(parsed) {
        register_binding(&app, binding.action, &shortcut)?;
      }
    }
  }
  Ok(())
}

#[tauri::command]
pub fn get_shortcut_settings(state: tauri::State<'_, ShortcutSettingsState>) -> ShortcutSettings {
  state
    .0
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .clone()
}

#[tauri::command]
pub fn set_shortcut_binding(
  app: AppHandle,
  state: tauri::State<'_, ShortcutSettingsState>,
  action: ShortcutAction,
  shortcut: Option<String>,
) -> Result<ShortcutSettings, String> {
  let shortcut = shortcut.filter(|value| !value.trim().is_empty());
  let mut settings = state
    .0
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .clone();
  let existing = settings
    .bindings
    .iter()
    .find(|binding| binding.action == action)
    .and_then(|binding| binding.shortcut.clone());

  let requested_id = shortcut
    .as_deref()
    .map(|value| value.parse::<Shortcut>().map(|value| value.id()))
    .transpose()
    .map_err(|error| error.to_string())?;

  if settings
    .bindings
    .iter()
    .filter(|binding| binding.action != action)
    .filter_map(|binding| binding.shortcut.as_deref())
    .filter_map(|value| value.parse::<Shortcut>().ok())
    .any(|value| Some(value.id()) == requested_id)
  {
    return Err("That shortcut is already assigned to another action".to_owned());
  }

  if let Some(existing) = existing.as_deref() {
    let _ = app.global_shortcut().unregister(existing);
  }
  if let Some(shortcut) = shortcut.as_deref() {
    if let Err(error) = register_binding(&app, action, shortcut) {
      if let Some(existing) = existing.as_deref() {
        let _ = register_binding(&app, action, existing);
      }
      return Err(error);
    }
  }

  if let Some(binding) = settings
    .bindings
    .iter_mut()
    .find(|binding| binding.action == action)
  {
    binding.shortcut = shortcut;
  }
  store(&app, &settings)?;
  *state
    .0
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner()) = settings.clone();
  crate::tray::refresh(&app);
  Ok(settings)
}

#[cfg(test)]
mod tests {
  use std::collections::HashSet;

  use super::*;

  #[test]
  fn defaults_open_the_recording_bar_take_screenshots_and_recognize_text() {
    let settings = ShortcutSettings::default();
    let assigned = settings
      .bindings
      .iter()
      .filter(|binding| binding.shortcut.is_some())
      .collect::<Vec<_>>();
    assert_eq!(assigned.len(), 3);
    assert_eq!(assigned[0].action, ShortcutAction::ToggleRecordingBar);
    assert_eq!(assigned[1].action, ShortcutAction::TakeScreenshot);
    assert_eq!(
      assigned[1].shortcut.as_deref(),
      Some("CommandOrControl+Shift+Digit8")
    );
    assert_eq!(assigned[2].action, ShortcutAction::RecognizeText);
    assert_eq!(
      assigned[2].shortcut.as_deref(),
      Some("CommandOrControl+Shift+KeyT")
    );
  }

  #[test]
  fn each_frontend_action_goes_to_the_window_that_performs_it() {
    assert_eq!(
      action_window(ShortcutAction::StartStopRecording).map(WindowLabel::as_str),
      Some(WindowLabel::RecordingBar.as_str())
    );
    assert_eq!(
      action_window(ShortcutAction::TakeScreenshot).map(WindowLabel::as_str),
      Some(WindowLabel::RegionSelector.as_str())
    );
  }

  #[test]
  fn taking_a_screenshot_never_reaches_the_recording_bar() {
    // The bar starts recordings, so the two must never share a window - and,
    // because emitting to a window does not scope delivery, the listeners
    // must go on matching the action itself.
    assert_ne!(
      action_window(ShortcutAction::TakeScreenshot).map(WindowLabel::as_str),
      action_window(ShortcutAction::StartStopRecording).map(WindowLabel::as_str)
    );
  }

  #[test]
  fn the_actions_rust_handles_alone_ask_no_window() {
    for action in [
      ShortcutAction::ToggleRecordingBar,
      ShortcutAction::PauseResumeRecording,
      ShortcutAction::RecognizeText,
    ] {
      assert!(action_window(action).is_none());
    }
  }

  #[test]
  fn every_action_appears_once() {
    let settings = ShortcutSettings::default();
    let actions = settings
      .bindings
      .iter()
      .map(|binding| binding.action)
      .collect::<HashSet<_>>();
    assert_eq!(actions.len(), settings.bindings.len());
  }
}
