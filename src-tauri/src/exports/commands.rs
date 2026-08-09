// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[tauri::command]
pub fn cancel_export(app: AppHandle) {
  discard(&app);
}

/// Requests cancellation of the save currently processing, if there is one.
///
/// The worker owns the FFmpeg child and performs the actual kill and wait. The
/// command only flips its token, so it never blocks the window thread or races
/// another thread for mutable access to the process.
#[tauri::command]
pub fn cancel_export_job(app: AppHandle) -> bool {
  let state = app.state::<ExportState>();
  let active = state
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
pub fn copy_export_to_clipboard(app: AppHandle) -> Result<(), String> {
  // Refused before the artifact is taken, not after: the clipboard cannot hold
  // a movie, and taking one only to put it back would drop its poster on the
  // way through. The window hides the button, so this is for callers that are
  // out of date rather than for anything a user can press.
  if matches!(
    app
      .state::<ExportState>()
      .artifact
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .as_ref(),
    Some(ExportArtifact::Recording { .. })
  ) {
    return Err("A recording cannot be copied to the clipboard".to_owned());
  }

  let artifact = take_artifact(&app).ok_or_else(|| "There is nothing to copy".to_owned())?;
  let ExportArtifact::Screenshot { image, .. } = artifact else {
    return Err("There is nothing to copy".to_owned());
  };

  app
    .clipboard()
    .write_image(&Image::new(&image.rgba, image.width, image.height))
    .map_err(|error| error.to_string())?;

  let _ = window::hide(&app);
  emit_snapshot(&app);

  Ok(())
}

#[tauri::command]
pub async fn browse_export_directory(app: AppHandle) -> Result<Option<PathBuf>, String> {
  let start = current_directory(&app);
  // Parented to the export window on purpose: left to itself the picker
  // attaches as a sheet to whichever window happens to be first, which for an
  // accessory app is usually one of the hidden overlay panels - and a sheet on
  // a hidden window is an invisible dialog.
  let parent = app.get_webview_window(crate::windows::WindowLabel::Export.as_str());
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
pub fn set_export_directory(app: AppHandle, directory: PathBuf) -> Result<(), String> {
  if !directory.is_dir() {
    return Err("That folder is no longer available".to_owned());
  }

  *app
    .state::<ExportState>()
    .directory
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(directory.clone());
  store_directory(&app, &directory).map_err(|error| error.to_string())?;
  emit_snapshot(&app);

  Ok(())
}
