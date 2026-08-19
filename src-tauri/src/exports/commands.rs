// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[tauri::command]
pub fn cancel_export(app: AppHandle, window: tauri::WebviewWindow) -> Result<(), String> {
  discard(&app, kind_of_window(&window)?);
  Ok(())
}

/// Brings the window holding a pending artifact to the front.
///
/// The recording bar keeps its capture buttons enabled while an export is
/// waiting, so pressing one has to lead somewhere: the same focus the global
/// shortcuts already fall back to. It names the workspace explicitly because
/// it is asking on another window's behalf, not its own.
#[tauri::command]
pub fn focus_export_window(app: AppHandle, kind: ExportKind) {
  super::workspace::focus_pending(&app, kind);
}

/// Requests cancellation of the save currently processing, if there is one.
///
/// The worker owns the FFmpeg child and performs the actual kill and wait. The
/// command only flips its token, so it never blocks the window thread or races
/// another thread for mutable access to the process.
#[tauri::command]
pub fn cancel_export_job(app: AppHandle, window: tauri::WebviewWindow) -> bool {
  let Ok(kind) = kind_of_window(&window) else {
    return false;
  };
  let state = app.state::<ExportState>();
  let active = state
    .slot(kind)
    .active_export
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());
  let Some(job) = active.as_ref() else {
    return false;
  };

  job.cancelled.store(true, Ordering::Release);
  true
}

#[tauri::command]
pub fn copy_export_to_clipboard(
  app: AppHandle,
  window: tauri::WebviewWindow,
  screenshot_output: ScreenshotWorkspaceOutputSettings,
) -> Result<(), String> {
  let kind = kind_of_window(&window)?;
  // Refused before the artifact is taken, not after: the clipboard cannot hold
  // a movie, and taking one only to put it back is pointless churn. The window
  // hides the button, so this is for callers that are out of date rather than
  // for anything a user can press.
  if matches!(
    app
      .state::<ExportState>()
      .slot(kind)
      .artifact
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .as_ref(),
    Some(ExportArtifact::Recording { .. })
  ) {
    return Err("A recording cannot be copied to the clipboard".to_owned());
  }

  let artifact = take_artifact(&app, kind).ok_or_else(|| "There is nothing to copy".to_owned())?;
  let ExportArtifact::Screenshot { items, .. } = artifact else {
    return Err("There is nothing to copy".to_owned());
  };
  let composed = compose_screenshot_workspace(&app, &items, &screenshot_output)?;

  app
    .clipboard()
    .write_image(&Image::new(&composed.rgba, composed.width, composed.height))
    .map_err(|error| error.to_string())?;
  if let Err(error) = remember_screenshot_output(&app, screenshot_output.canvas) {
    eprintln!("Could not remember screenshot export settings: {error}");
  }

  let _ = window::hide(&app, kind);
  emit_snapshot(&app, kind);

  Ok(())
}

#[tauri::command]
pub fn set_screenshot_radius(app: AppHandle, radius_percent: f64) -> Result<(), String> {
  remember_screenshot_radius(&app, radius_percent).map(|_| ())
}

#[tauri::command]
pub fn set_screenshot_background_radius(app: AppHandle, radius_percent: f64) -> Result<(), String> {
  remember_screenshot_background_radius(&app, radius_percent).map(|_| ())
}

#[tauri::command]
pub async fn browse_export_directory(
  app: AppHandle,
  window: tauri::WebviewWindow,
) -> Result<Option<PathBuf>, String> {
  let start = current_directory(&app, kind_of_window(&window)?);
  // Parented to the asking window on purpose: left to itself the picker
  // attaches as a sheet to whichever window happens to be first, which for an
  // accessory app is usually one of the hidden overlay panels - and a sheet on
  // a hidden window is an invisible dialog.
  let parent = Some(window);
  let picked = tauri::async_runtime::spawn_blocking(move || {
    use tauri_plugin_dialog::DialogExt;

    let mut dialog = app.dialog().file().set_title("Choose a folder");
    if let Some(start) = start {
      dialog = dialog.set_directory(start);
    }
    if let Some(parent) = &parent {
      dialog = dialog.set_parent(parent);
    }
    dialog.blocking_pick_folder()
  })
  .await
  .map_err(|error| error.to_string())?;

  Ok(picked.and_then(|path| path.into_path().ok()))
}

#[tauri::command]
pub fn set_export_directory(
  app: AppHandle,
  window: tauri::WebviewWindow,
  directory: PathBuf,
) -> Result<(), String> {
  store_export_directory(&app, kind_of_window(&window)?, directory)
}

/// Remembers where one workspace saves. Split out of the command so a finished
/// save can record its own destination without a window to derive it from.
pub(super) fn store_export_directory(
  app: &AppHandle,
  kind: ExportKind,
  directory: PathBuf,
) -> Result<(), String> {
  if !directory.is_dir() {
    return Err("That folder is no longer available".to_owned());
  }

  *app
    .state::<ExportState>()
    .slot(kind)
    .directory
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(directory);
  emit_snapshot(app, kind);

  Ok(())
}
