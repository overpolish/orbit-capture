use tauri::WebviewWindow;

#[cfg(target_os = "macos")]
use tauri::Manager;

#[cfg(target_os = "macos")]
use tauri_nspanel::{
  tauri_panel, CollectionBehavior, PanelLevel, StyleMask, TrackingAreaOptions, WebviewWindowExt,
};

#[cfg(target_os = "macos")]
tauri_panel! {
  panel!(RecordingBarPanel {
    config: {
      can_become_key_window: true,
      can_become_main_window: false,
      becomes_key_only_if_needed: true,
      hides_on_deactivate: false,
      is_floating_panel: true,
      works_when_modal: true
    }
    with: {
      tracking_area: {
        options: TrackingAreaOptions::new()
          .active_always()
          .mouse_entered_and_exited()
          .mouse_moved()
          .cursor_update(),
        auto_resize: true
      }
    }
  })
}

#[cfg(target_os = "macos")]
pub fn initialize_recording_bar(window: &WebviewWindow) -> tauri::Result<()> {
  let panel = window.to_panel::<RecordingBarPanel>()?;

  panel.set_level(PanelLevel::Custom(28).value());
  panel.set_style_mask(StyleMask::empty().nonactivating_panel().into());
  panel.set_collection_behavior(
    CollectionBehavior::new()
      .full_screen_auxiliary()
      .can_join_all_spaces()
      .transient()
      .into(),
  );
  panel.set_hides_on_deactivate(false);
  panel.set_works_when_modal(true);
  panel.set_accepts_mouse_moved_events(true);
  window.hide()?;

  Ok(())
}

#[cfg(target_os = "windows")]
pub fn initialize_recording_bar(window: &WebviewWindow) -> tauri::Result<()> {
  window.set_skip_taskbar(true)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn initialize_recording_bar(_window: &WebviewWindow) -> tauri::Result<()> {
  Ok(())
}
