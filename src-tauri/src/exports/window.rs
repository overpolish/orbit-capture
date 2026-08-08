use tauri::utils::config::WindowEffectsConfig;
use tauri::window::{Effect, EffectState};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::windows::{self, WindowLabel};

const WIDTH: f64 = 460.0;
/// A close-enough first paint. The window resizes itself to the content's
/// measured height as soon as it mounts, so this only avoids a visible jump.
const HEIGHT: f64 = 530.0;

/// Built at runtime rather than declared in `tauri.conf.json`, like the
/// permissions window: it is an ordinary focusable window, and deliberately
/// never a nonactivating panel, because a panel cannot hold a text field.
pub fn show(app: &AppHandle) -> tauri::Result<()> {
  let window = windows::get_or_create(app, WindowLabel::Export, || {
    WebviewWindowBuilder::new(
      app,
      WindowLabel::Export.as_str(),
      WebviewUrl::App("/export".into()),
    )
    .title("Orbit Capture Export")
    .inner_size(WIDTH, HEIGHT)
    .center()
    .always_on_top(true)
    .decorations(false)
    .resizable(false)
    .shadow(true)
    .skip_taskbar(true)
    .transparent(true)
    .effects(WindowEffectsConfig {
      color: None,
      // `UnderWindowBackground` is macOS-only; `Mica` is its Windows
      // counterpart. Listing both lets each platform pick the one it honors —
      // without Mica, the transparent window has no backdrop on Windows and
      // shows through wherever the web content isn't fully opaque.
      effects: vec![Effect::UnderWindowBackground, Effect::Mica],
      radius: Some(10.0),
      state: Some(EffectState::Active),
    })
    .build()
    .inspect(|window| {
      // Registered on creation only: `get_or_create` hands back the same
      // window afterwards, and stacking handlers would hide it repeatedly.
      let _ = windows::initialize_export(window);
      windows::hide_instead_of_close(app, WindowLabel::Export);
    })
  })?;

  windows::show(&window, true)?;
  // Re-asserted on every show, not just at creation: the overlays raise
  // themselves too, so the ordering has to be claimed each time.
  windows::raise_export(&window)
}

pub fn hide(app: &AppHandle) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(WindowLabel::Export.as_str()) {
    window.hide()?;
  }

  Ok(())
}
