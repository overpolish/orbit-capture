mod permissions;
mod recording_sources;

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
      recording_sources::center_window,
      recording_sources::list_monitors,
      recording_sources::list_windows,
      recording_sources::make_window_borderless,
      recording_sources::resize_window,
      recording_sources::restore_window_border,
      windows::collapse_recording_source_selector,
      windows::finish_recording_bar_drag,
      windows::hide_recording_ui,
      windows::hide_region_selector,
      windows::set_recording_controls_opacity,
      windows::set_recording_source_selector_visible,
      windows::set_region_selector_opacity,
      windows::set_region_selector_passthrough,
      windows::show_region_selector,
      windows::take_monitor_screenshot,
      windows::toggle_recording_source_selector,
    ])
    .setup(|app| {
      #[cfg(target_os = "macos")]
      app.set_activation_policy(tauri::ActivationPolicy::Accessory);

      #[cfg(desktop)]
      tray::initialize(app)?;

      windows::initialize_recording_bar(app.handle())?;
      windows::initialize_recording_source_selector(app.handle())?;
      windows::initialize_region_selector(app.handle())?;
      windows::hide_instead_of_close(app.handle(), windows::WindowLabel::RecordingBar);
      windows::hide_instead_of_close(app.handle(), windows::WindowLabel::RecordingSourceSelector);
      windows::hide_instead_of_close(app.handle(), windows::WindowLabel::RegionSelector);
      windows::initialize_recording_bar_position(app.handle())?;
      windows::manage_recording_bar_movement(app.handle());
      windows::manage_recording_source_selector_dismissal(app.handle());

      #[cfg(target_os = "macos")]
      {
        let snapshot = tauri::async_runtime::block_on(permissions::refresh(app.handle()));
        if !snapshot.has_required_recording_permissions() {
          permissions::show_permissions_window(app.handle())?;
        } else {
          windows::show_recording_ui(app.handle())?;
        }
      }

      #[cfg(not(target_os = "macos"))]
      windows::show_recording_ui(app.handle())?;

      permissions::start_watcher(app.handle().clone());

      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
