mod models;

#[cfg(target_os = "macos")]
mod window;

use std::sync::RwLock;
#[cfg(target_os = "macos")]
use std::{collections::HashSet, sync::Mutex};

use models::{PermissionKind, PermissionSnapshot};
#[cfg(target_os = "macos")]
use tauri::Emitter;
use tauri::{AppHandle, Manager, State};

#[cfg(target_os = "macos")]
const PERMISSIONS_CHANGED_EVENT: &str = "permissions://changed";

pub struct PermissionState {
  #[cfg(target_os = "macos")]
  requested: Mutex<HashSet<PermissionKind>>,
  snapshot: RwLock<PermissionSnapshot>,
}

impl Default for PermissionState {
  fn default() -> Self {
    Self {
      #[cfg(target_os = "macos")]
      requested: Mutex::default(),
      snapshot: RwLock::new(if cfg!(target_os = "macos") {
        PermissionSnapshot::unavailable()
      } else {
        PermissionSnapshot::granted()
      }),
    }
  }
}

pub fn cached_snapshot(app: &AppHandle) -> PermissionSnapshot {
  app
    .state::<PermissionState>()
    .snapshot
    .read()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .clone()
}

#[cfg(target_os = "macos")]
pub fn has_required_recording_permissions(app: &AppHandle) -> bool {
  cached_snapshot(app).has_required_recording_permissions()
}

#[cfg(target_os = "macos")]
pub async fn refresh(app: &AppHandle) -> PermissionSnapshot {
  use tauri_plugin_macos_permissions::{
    check_accessibility_permission, check_camera_permission, check_microphone_permission,
    check_screen_recording_permission,
  };

  let (accessibility, screen_recording, camera, microphone) = tokio::join!(
    check_accessibility_permission(),
    check_screen_recording_permission(),
    check_camera_permission(),
    check_microphone_permission(),
  );
  let state = app.state::<PermissionState>();
  let requested = state
    .requested
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());
  let snapshot = PermissionSnapshot {
    accessibility: models::PermissionStatus {
      can_request: true,
      granted: accessibility,
    },
    screen_recording: models::PermissionStatus {
      can_request: true,
      granted: screen_recording,
    },
    camera: models::PermissionStatus {
      can_request: true,
      granted: camera,
    },
    microphone: models::PermissionStatus {
      can_request: true,
      granted: microphone,
    },
  }
  .with_request_state(&requested);
  drop(requested);

  let changed = {
    let mut current = state
      .snapshot
      .write()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    let changed = *current != snapshot;
    *current = snapshot.clone();
    changed
  };

  if changed {
    let _ = app.emit(PERMISSIONS_CHANGED_EVENT, &snapshot);
  }

  snapshot
}

#[cfg(not(target_os = "macos"))]
pub async fn refresh(app: &AppHandle) -> PermissionSnapshot {
  cached_snapshot(app)
}

#[cfg(target_os = "macos")]
pub fn start_watcher(app: AppHandle) {
  tauri::async_runtime::spawn(async move {
    loop {
      refresh(&app).await;
      tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
  });
}

#[cfg(not(target_os = "macos"))]
pub fn start_watcher(_app: AppHandle) {}

#[tauri::command]
pub fn permission_snapshot(state: State<'_, PermissionState>) -> PermissionSnapshot {
  state
    .snapshot
    .read()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .clone()
}

#[tauri::command]
pub async fn request_permission(app: AppHandle, permission: PermissionKind) -> Result<(), String> {
  #[cfg(target_os = "macos")]
  {
    use tauri_plugin_macos_permissions::{
      request_accessibility_permission, request_camera_permission, request_microphone_permission,
      request_screen_recording_permission,
    };

    app
      .state::<PermissionState>()
      .requested
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .insert(permission);

    match permission {
      PermissionKind::Accessibility => request_accessibility_permission().await,
      PermissionKind::ScreenRecording => request_screen_recording_permission().await,
      PermissionKind::Camera => request_camera_permission().await?,
      PermissionKind::Microphone => request_microphone_permission().await?,
    }

    refresh(&app).await;
  }

  #[cfg(not(target_os = "macos"))]
  let _ = (app, permission);

  Ok(())
}

#[tauri::command]
pub fn open_permission_settings(permission: PermissionKind) -> Result<(), String> {
  #[cfg(target_os = "macos")]
  {
    let pane = match permission {
      PermissionKind::Accessibility => "Privacy_Accessibility",
      PermissionKind::ScreenRecording => "Privacy_ScreenCapture",
      PermissionKind::Camera => "Privacy_Camera",
      PermissionKind::Microphone => "Privacy_Microphone",
    };

    std::process::Command::new("open")
      .arg(format!(
        "x-apple.systempreferences:com.apple.preference.security?{pane}"
      ))
      .spawn()
      .map_err(|error| error.to_string())?;
  }

  #[cfg(not(target_os = "macos"))]
  let _ = permission;

  Ok(())
}

#[tauri::command]
pub async fn require_permissions(
  app: AppHandle,
  required: Vec<PermissionKind>,
) -> Result<(), Vec<PermissionKind>> {
  let snapshot = refresh(&app).await;
  let missing = snapshot.missing(&required);

  if missing.is_empty() {
    Ok(())
  } else {
    Err(missing)
  }
}

#[tauri::command]
pub fn restart_app(app: AppHandle) {
  app.restart();
}

#[cfg(target_os = "macos")]
pub fn show_permissions_window(app: &AppHandle) -> tauri::Result<()> {
  window::show(app)
}
