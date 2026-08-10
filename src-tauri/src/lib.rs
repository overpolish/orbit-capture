// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

mod audio_preview;
mod camera_format;
mod camera_frames;
mod camera_preview;
mod capture_geometry;
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

use tauri::Manager;

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

  let mut app = builder
    .manage(audio_preview::AudioPreviewState::default())
    .manage(camera_preview::CameraPreviewState::default())
    .manage(exports::ExportState::default())
    .manage(exports::recording_preview_player::RecordingPreviewPlayerState::default())
    .manage(permissions::PermissionState::default())
    .manage(recording::RecordingState::default())
    .invoke_handler(tauri::generate_handler![
      audio_preview::start_audio_preview,
      audio_preview::stop_audio_preview,
      camera_preview::start_camera_preview,
      camera_preview::stop_camera_preview,
      exports::commands::browse_export_directory,
      exports::commands::cancel_export,
      exports::commands::cancel_export_job,
      exports::commands::copy_export_to_clipboard,
      exports::preview::estimate_recording_export,
      exports::preview::get_export_preview,
      exports::preview::get_export_snapshot,
      exports::recording_preview::get_recording_preview,
      exports::recording_preview_player::commands::pause_recording_preview,
      exports::recording_preview_player::commands::play_recording_preview,
      exports::recording_preview_player::commands::request_recording_preview_full_resolution,
      exports::recording_preview_player::commands::seek_recording_preview,
      exports::recording_preview_player::commands::select_recording_preview_audio,
      exports::recording_preview_player::commands::start_recording_preview_player,
      exports::recording_preview_player::commands::stop_recording_preview_player,
      exports::save::save_export,
      exports::commands::set_export_directory,
      exports::commands::set_screenshot_radius,
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
      windows::dock::finish_recording_dock_drag,
      windows::options::hide_recording_options,
      windows::hide_recording_ui,
      windows::region::hide_region_selector,
      windows::options::hide_standalone_listbox,
      windows::region::set_recording_controls_opacity,
      windows::set_recording_source_selector_visible,
      windows::region::set_region_selector_opacity,
      windows::region::set_region_selector_passthrough,
      windows::region::show_region_selector,
      windows::options::show_standalone_listbox,
      windows::monitor_capture::take_monitor_screenshot,
      windows::toggle_recording_source_selector,
      windows::options::toggle_recording_options,
    ])
    .setup(|app| {
      #[cfg(desktop)]
      tray::initialize(app)?;

      windows::initialize_recording_bar(app.handle())?;
      windows::initialize_recording_source_selector(app.handle())?;
      windows::initialize_region_selector(app.handle())?;
      windows::initialize_recording_options(app.handle())?;
      windows::initialize_standalone_listbox(app.handle())?;
      windows::initialize_recording_dock(app.handle())?;
      if let Some(window) = app.get_webview_window(windows::WindowLabel::Export.as_str()) {
        windows::initialize_export(&window)?;
      }
      windows::hide_instead_of_close(app.handle(), windows::WindowLabel::RecordingBar);
      windows::hide_instead_of_close(app.handle(), windows::WindowLabel::RecordingSourceSelector);
      windows::hide_instead_of_close(app.handle(), windows::WindowLabel::RegionSelector);
      windows::hide_instead_of_close(app.handle(), windows::WindowLabel::RecordingOptions);
      windows::hide_instead_of_close(app.handle(), windows::WindowLabel::StandaloneListbox);
      windows::hide_instead_of_close(app.handle(), windows::WindowLabel::RecordingDock);
      windows::hide_instead_of_close(app.handle(), windows::WindowLabel::Export);
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

      #[cfg(target_os = "macos")]
      {
        // Native window effects finish after `setup` returns and can order the
        // configured export window onscreen. Its first presentation always
        // belongs to an actual capture.
        let app_handle = app.handle().clone();
        app.handle().run_on_main_thread(move || {
          if let Some(export) = app_handle.get_webview_window(windows::WindowLabel::Export.as_str())
          {
            let _ = export.hide();
          }
        })?;
      }

      Ok(())
    })
    .build(tauri::generate_context!())
    .expect("error while running tauri application");

  #[cfg(target_os = "macos")]
  app.set_dock_visibility(false);

  app.run(|_, _| {});
}
