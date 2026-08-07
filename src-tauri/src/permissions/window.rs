use tauri::utils::config::WindowEffectsConfig;
use tauri::window::{Effect, EffectState};
use tauri::{AppHandle, WebviewUrl, WebviewWindowBuilder};

use crate::windows::{self, WindowLabel};

pub fn show(app: &AppHandle) -> tauri::Result<()> {
  let window = windows::get_or_create(app, WindowLabel::Permissions, || {
    WebviewWindowBuilder::new(
      app,
      WindowLabel::Permissions.as_str(),
      WebviewUrl::App("/permissions".into()),
    )
    .title("Orbit Capture Permissions")
    .inner_size(540.0, 432.0)
    .center()
    .always_on_top(true)
    .closable(false)
    .decorations(false)
    .resizable(false)
    .shadow(true)
    .skip_taskbar(true)
    .transparent(true)
    .effects(WindowEffectsConfig {
      color: None,
      effects: vec![Effect::UnderWindowBackground],
      radius: Some(10.0),
      state: Some(EffectState::Active),
    })
    .build()
  })?;

  windows::show(&window)
}
