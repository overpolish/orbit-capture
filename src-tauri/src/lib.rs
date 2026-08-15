// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

mod audio_preview;
mod camera_format;
mod camera_frames;
mod camera_preview;
mod capture_geometry;
#[cfg(target_os = "macos")]
mod capture_kit;
#[cfg(target_os = "macos")]
mod cursor_scrub;
mod exports;
mod permissions;
mod recording;
mod recording_inputs;
mod recording_sources;
mod screenshots;
mod settings;
mod shortcuts;

mod text_recognition;
#[cfg(desktop)]
mod tray;
mod windows;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  let builder = tauri::Builder::default()
    .plugin(tauri_plugin_autostart::init(
      tauri_plugin_autostart::MacosLauncher::LaunchAgent,
      None,
    ))
    .plugin(tauri_plugin_clipboard_manager::init())
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_global_shortcut::Builder::new().build())
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

  let app = builder
    .manage(audio_preview::AudioPreviewState::default())
    .manage(camera_preview::CameraPreviewState::default())
    .manage(exports::ExportState::default())
    .manage(exports::recording_preview_player::RecordingPreviewPlayerState::default())
    .manage(exports::screenshot_preview::ScreenshotPreviewState::default())
    .manage(permissions::PermissionState::default())
    .manage(recording::RecordingState::default())
    .manage(settings::GeneralSettingsState::default())
    .manage(shortcuts::ShortcutSettingsState::default())
    .manage(text_recognition::TextRecognitionState::default())
    .invoke_handler(tauri::generate_handler![
      audio_preview::start_audio_preview,
      audio_preview::stop_audio_preview,
      camera_preview::start_camera_preview,
      camera_preview::stop_camera_preview,
      #[cfg(target_os = "macos")]
      cursor_scrub::begin_cursor_scrub,
      #[cfg(target_os = "macos")]
      cursor_scrub::end_cursor_scrub,
      exports::commands::browse_export_directory,
      exports::commands::cancel_export,
      exports::commands::cancel_export_job,
      exports::commands::copy_export_to_clipboard,
      exports::preview::estimate_recording_export,
      exports::preview::get_export_preview,
      exports::preview::render_screenshot_output_preview,
      exports::preview::get_export_snapshot,
      exports::preview_platform::preview_capabilities,
      exports::recording_preview::get_recording_preview,
      exports::recording_preview_player::commands::pause_recording_preview,
      exports::recording_preview_player::surface_commands::layout_recording_preview_surface,
      exports::recording_preview_player::commands::play_recording_preview,
      exports::recording_preview_player::commands::request_recording_preview_full_resolution,
      exports::recording_preview_player::commands::seek_recording_preview,
      exports::recording_preview_player::commands::select_recording_preview_audio,
      exports::recording_preview_player::commands::set_recording_preview_audio_volumes,
      exports::recording_preview_player::commands::set_recording_preview_cursor_effects,
      exports::recording_preview_player::commands::set_recording_preview_composition,
      exports::recording_preview_player::commands::start_recording_preview_player,
      exports::recording_preview_player::commands::stop_recording_preview_player,
        exports::recording_preview_player::timeline_thumbnails::copy_recording_preview_source_frame,
        exports::recording_preview_player::timeline_thumbnails::copy_recording_preview_frame_to_clipboard,
      exports::recording_preview_player::timeline_thumbnails::stream_recording_timeline_thumbnails,
      exports::save::save_export,
      exports::screenshot_preview::layout_screenshot_preview_surface,
      exports::screenshot_preview::set_screenshot_preview_output,
      exports::screenshot_preview::start_screenshot_preview,
      exports::screenshot_preview::stop_screenshot_preview,
      exports::commands::set_export_directory,
      exports::commands::set_screenshot_background_radius,
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
      recording::start_recording_monitor,
      recording::start_recording,
      recording::stop_recording_monitor,
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
      text_recognition::cancel_text_recognition,
      text_recognition::capture_text_region,
      text_recognition::copy_recognized_text,
      text_recognition::recognize_captured_text,
      text_recognition::start_text_recognition,
      settings::hide_settings,
      settings::preferences::browse_default_location,
      settings::preferences::get_general_settings,
      settings::preferences::set_general_settings,
      settings::show_settings,
      shortcuts::get_shortcut_settings,
      shortcuts::begin_shortcut_capture,
      shortcuts::end_shortcut_capture,
      shortcuts::set_shortcut_binding,
      windows::collapse_recording_source_selector,
      windows::finish_recording_bar_drag,
      windows::dock::finish_recording_dock_drag,
      windows::dock::resize_recording_dock,
      windows::options::hide_recording_options,
      windows::hide_recording_ui,
      windows::recording_ui_visible,
      windows::region::hide_region_selector,
      windows::options::hide_standalone_listbox,
      windows::region::set_recording_controls_opacity,
      windows::set_recording_source_selector_visible,
      windows::region::set_region_selector_opacity,
      windows::region::set_region_selector_passthrough,
      windows::region::set_screenshot_region_session,
      windows::region::show_region_selector,
      windows::options::show_standalone_listbox,
      windows::monitor_capture::take_monitor_screenshot,
      windows::toggle_recording_source_selector,
      windows::options::toggle_recording_options,
    ])
    .setup(|app| {
      #[cfg(target_os = "macos")]
      {
        exports::initialize_cursor_artwork();
      }

      #[cfg(desktop)]
      tray::initialize(app)?;

      settings::initialize(app.handle());
      let show_recording_bar_on_launch =
        settings::current(app.handle()).show_recording_bar_on_launch;

      // Converting a hidden macOS webview into an NSPanel can order one stale
      // compositor frame onscreen. Keep tray-only startup genuinely tray-only:
      // panels are created lazily by the first explicit show request instead.
      #[cfg(not(target_os = "macos"))]
      {
        windows::initialize_recording_bar(app.handle())?;
        windows::initialize_recording_source_selector(app.handle())?;
        windows::initialize_region_selector(app.handle())?;
        windows::initialize_recording_options(app.handle())?;
        windows::initialize_standalone_listbox(app.handle())?;
        windows::initialize_recording_dock(app.handle())?;
      }
      if let Some(window) = app.get_webview_window(windows::WindowLabel::Settings.as_str()) {
        windows::initialize_normal_window(&window)?;
      }
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
      windows::hide_instead_of_close(app.handle(), windows::WindowLabel::Settings);
      windows::initialize_recording_bar_position(app.handle())?;
      windows::manage_recording_bar_movement(app.handle());
      windows::manage_recording_dock_movement(app.handle());
      exports::initialize(app.handle());
      shortcuts::initialize(app.handle());
      windows::manage_recording_source_selector_dismissal(app.handle());

      #[cfg(target_os = "macos")]
      {
        let snapshot = tauri::async_runtime::block_on(permissions::refresh(app.handle()));
        if !snapshot.has_required_recording_permissions() {
          permissions::show_permissions_window(app.handle())?;
        } else if show_recording_bar_on_launch {
          windows::show_recording_ui(app.handle())?;
        }
      }

      #[cfg(not(target_os = "macos"))]
      if show_recording_bar_on_launch {
        windows::show_recording_ui(app.handle())?;
      }

      permissions::start_watcher(app.handle().clone());

      #[cfg(target_os = "macos")]
      {
        // Native effects can order ordinary app windows during setup. Their
        // first presentation always belongs to an explicit user action.
        let app_handle = app.handle().clone();
        app.handle().run_on_main_thread(move || {
          for label in [windows::WindowLabel::Export, windows::WindowLabel::Settings] {
            if let Some(window) = app_handle.get_webview_window(label.as_str()) {
              let _ = window.hide();
            }
          }
        })?;
      }

      Ok(())
    })
    .build(tauri::generate_context!())
    .expect("error while running tauri application");

  #[cfg(target_os = "macos")]
  let mut app = app;

  #[cfg(target_os = "macos")]
  app.set_dock_visibility(false);

  app.run(|_, _| {});
}
