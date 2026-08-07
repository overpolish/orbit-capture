mod permissions;

#[cfg(desktop)]
mod tray;
mod windows;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  let builder = tauri::Builder::default().plugin(
    tauri_plugin_window_state::Builder::default()
      .with_state_flags(tauri_plugin_window_state::StateFlags::POSITION)
      .with_filter(|label| label == windows::WindowLabel::RecordingBar.as_str())
      .skip_initial_state(windows::WindowLabel::RecordingBar.as_str())
      .build(),
  );

  #[cfg(target_os = "macos")]
  let builder = builder
    .plugin(tauri_plugin_macos_permissions::init())
    .plugin(tauri_nspanel::init());

  builder
    .manage(permissions::PermissionState::default())
    .invoke_handler(tauri::generate_handler![
      permissions::open_permission_settings,
      permissions::permission_snapshot,
      permissions::request_permission,
      permissions::require_permissions,
      permissions::restart_app,
    ])
    .setup(|app| {
      #[cfg(target_os = "macos")]
      app.set_activation_policy(tauri::ActivationPolicy::Accessory);

      #[cfg(desktop)]
      tray::initialize(app)?;

      windows::initialize_recording_bar(app.handle())?;
      windows::hide_instead_of_close(app.handle(), windows::WindowLabel::RecordingBar);
      windows::initialize_recording_bar_position(app.handle())?;

      #[cfg(target_os = "macos")]
      {
        let snapshot = tauri::async_runtime::block_on(permissions::refresh(app.handle()));
        if !snapshot.has_required_recording_permissions() {
          permissions::show_permissions_window(app.handle())?;
        } else {
          windows::show_existing(app.handle(), windows::WindowLabel::RecordingBar, false)?;
        }
      }

      #[cfg(not(target_os = "macos"))]
      windows::show_existing(app.handle(), windows::WindowLabel::RecordingBar, false)?;

      permissions::start_watcher(app.handle().clone());

      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
