// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

mod audio_preview;
mod camera_preview;
#[cfg(target_os = "macos")]
mod capture_kit;
mod exports;
mod permissions;
mod recording;
mod recording_inputs;
mod recording_sources;
mod screenshots;

#[cfg(desktop)]
mod tray;
mod windows;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  let builder = tauri::Builder::default()
    .plugin(tauri_plugin_clipboard_manager::init())
    .plugin(tauri_plugin_dialog::init())
    .plugin(
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
    .manage(audio_preview::AudioPreviewState::default())
    .manage(camera_preview::CameraPreviewState::default())
    .manage(exports::ExportState::default())
    .manage(permissions::PermissionState::default())
    .manage(recording::RecordingState::default())
    .invoke_handler(tauri::generate_handler![
      audio_preview::start_audio_preview,
      audio_preview::stop_audio_preview,
      camera_preview::start_camera_preview,
      camera_preview::stop_camera_preview,
      exports::browse_export_directory,
      exports::cancel_export,
      exports::cancel_export_job,
      exports::copy_export_to_clipboard,
      exports::estimate_recording_export,
      exports::get_export_preview,
      exports::get_export_snapshot,
      exports::get_recording_preview,
      exports::get_recording_preview_mix,
      exports::save_export,
      exports::set_export_directory,
      permissions::open_permission_settings,
      permissions::permission_snapshot,
      permissions::request_permission,
      permissions::require_permissions,
      permissions::restart_app,
      recording::cancel_recording,
      recording::get_recording_snapshot,
      recording::pause_recording,
      recording::resume_recording,
      recording::start_recording,
      recording::stop_recording,
      recording_inputs::list_cameras,
      recording_inputs::list_microphones,
      recording_sources::center_window,
      recording_sources::list_applications,
      recording_sources::list_monitors,
      recording_sources::list_windows,
      recording_sources::make_window_borderless,
      recording_sources::resize_window,
      recording_sources::restore_window_border,
      screenshots::capture_still,
      windows::collapse_recording_source_selector,
      windows::finish_recording_bar_drag,
      windows::finish_recording_dock_drag,
      windows::hide_recording_options,
      windows::hide_recording_ui,
      windows::hide_region_selector,
      windows::hide_standalone_listbox,
      windows::set_recording_controls_opacity,
      windows::set_recording_source_selector_visible,
      windows::set_region_selector_opacity,
      windows::set_region_selector_passthrough,
      windows::show_region_selector,
      windows::show_standalone_listbox,
      windows::take_monitor_screenshot,
      windows::toggle_recording_source_selector,
      windows::toggle_recording_options,
    ])
    .setup(|app| {
      #[cfg(target_os = "macos")]
      app.set_activation_policy(tauri::ActivationPolicy::Accessory);

      #[cfg(desktop)]
      tray::initialize(app)?;

      windows::initialize_recording_bar(app.handle())?;
      windows::initialize_recording_source_selector(app.handle())?;
      windows::initialize_region_selector(app.handle())?;
      windows::initialize_recording_options(app.handle())?;
      windows::initialize_standalone_listbox(app.handle())?;
      windows::initialize_recording_dock(app.handle())?;
      windows::hide_instead_of_close(app.handle(), windows::WindowLabel::RecordingBar);
      windows::hide_instead_of_close(app.handle(), windows::WindowLabel::RecordingSourceSelector);
      windows::hide_instead_of_close(app.handle(), windows::WindowLabel::RegionSelector);
      windows::hide_instead_of_close(app.handle(), windows::WindowLabel::RecordingOptions);
      windows::hide_instead_of_close(app.handle(), windows::WindowLabel::StandaloneListbox);
      windows::hide_instead_of_close(app.handle(), windows::WindowLabel::RecordingDock);
      windows::initialize_recording_bar_position(app.handle())?;
      windows::manage_recording_bar_movement(app.handle());
      windows::manage_recording_dock_movement(app.handle());
      exports::initialize(app.handle());
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
