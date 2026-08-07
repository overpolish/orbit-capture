use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;
use tauri::{
  AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, PhysicalPosition, WebviewWindow,
  WindowEvent,
};
use tauri_plugin_window_state::{AppHandleExt, StateFlags, WindowExt};

mod platform;

#[derive(Clone, Copy)]
pub enum WindowLabel {
  Permissions,
  RecordingBar,
  RecordingSourceSelector,
}

impl WindowLabel {
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Permissions => "permissions",
      Self::RecordingBar => "recording-bar",
      Self::RecordingSourceSelector => "recording-source-selector",
    }
  }
}

pub fn get_or_create<F>(
  app: &AppHandle,
  label: WindowLabel,
  create: F,
) -> tauri::Result<WebviewWindow>
where
  F: FnOnce() -> tauri::Result<WebviewWindow>,
{
  app
    .get_webview_window(label.as_str())
    .map_or_else(create, Ok)
}

pub fn show(window: &WebviewWindow, focus: bool) -> tauri::Result<()> {
  window.show()?;
  window.unminimize()?;
  if focus {
    window.set_focus()?;
  }

  Ok(())
}

pub fn initialize_recording_bar_position(app: &AppHandle) -> tauri::Result<()> {
  let Some(window) = app.get_webview_window(WindowLabel::RecordingBar.as_str()) else {
    return Ok(());
  };
  let Some(monitor) = window.current_monitor()? else {
    return Ok(());
  };

  let monitor_position = monitor.position();
  let monitor_size = monitor.size();
  let window_size = window.outer_size()?;

  window.set_position(PhysicalPosition {
    x: monitor_position.x + (monitor_size.width.saturating_sub(window_size.width) / 2) as i32,
    y: monitor_position.y + monitor_size.height.saturating_sub(window_size.height + 100) as i32,
  })?;

  // Restoring after the fallback means the first launch has a sensible
  // position while later launches respect where the user moved the bar.
  let _ = window.restore_state(StateFlags::POSITION);
  keep_window_on_a_monitor(app, &window)?;

  Ok(())
}

fn keep_window_on_a_monitor(app: &AppHandle, window: &WebviewWindow) -> tauri::Result<()> {
  let window_position = window.outer_position()?;
  let window_size = window.outer_size()?;
  let is_visible = app.available_monitors()?.iter().any(|monitor| {
    let position = monitor.position();
    let size = monitor.size();
    let left = window_position.x.max(position.x);
    let top = window_position.y.max(position.y);
    let right = (window_position.x + window_size.width as i32).min(position.x + size.width as i32);
    let bottom =
      (window_position.y + window_size.height as i32).min(position.y + size.height as i32);

    right > left && bottom > top
  });

  if !is_visible {
    if let Some(monitor) = app.primary_monitor()? {
      let position = monitor.position();
      let size = monitor.size();
      window.set_position(PhysicalPosition {
        x: position.x + (size.width.saturating_sub(window_size.width) / 2) as i32,
        y: position.y + size.height.saturating_sub(window_size.height + 100) as i32,
      })?;
    }
  }

  Ok(())
}

pub fn initialize_recording_bar(app: &AppHandle) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(WindowLabel::RecordingBar.as_str()) {
    platform::initialize_recording_bar(&window)?;
  }

  Ok(())
}

pub fn initialize_recording_source_selector(app: &AppHandle) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(WindowLabel::RecordingSourceSelector.as_str()) {
    platform::initialize_recording_source_selector(&window)?;
  }

  Ok(())
}

const SELECTOR_COLLAPSED_WIDTH: f64 = 300.0;
const SELECTOR_COLLAPSED_HEIGHT: f64 = 40.0;
const SELECTOR_EXPANDED_WIDTH: f64 = 500.0;
const SELECTOR_EXPANDED_HEIGHT: f64 = 250.0;
const SELECTOR_GAP: f64 = 6.0;
const ANIMATION_STEPS: u64 = 18;
const BAR_DRAG_SETTLE_DELAY: Duration = Duration::from_millis(200);
static SELECTOR_ANIMATION: AtomicU64 = AtomicU64::new(0);
static BAR_MOVE_SETTLE: AtomicU64 = AtomicU64::new(0);
static SELECTOR_EXPANDED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum SelectorPlacement {
  Above,
  Below,
}

struct SelectorFrame {
  position: LogicalPosition<f64>,
  size: LogicalSize<f64>,
}

fn selector_frames(
  app: &AppHandle,
) -> tauri::Result<(SelectorPlacement, SelectorFrame, SelectorFrame)> {
  let bar = app
    .get_webview_window(WindowLabel::RecordingBar.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  let bar_position = bar.outer_position()?;
  let bar_size = bar.outer_size()?;
  let monitor = bar
    .current_monitor()?
    .or(app.primary_monitor()?)
    .ok_or_else(|| tauri::Error::WindowNotFound)?;

  let scale = monitor.scale_factor();
  let monitor_position = monitor.position().to_logical::<f64>(scale);
  let monitor_size = monitor.size().to_logical::<f64>(scale);
  let bar_position = bar_position.to_logical::<f64>(scale);
  let bar_size = bar_size.to_logical::<f64>(scale);
  let monitor_right = monitor_position.x + monitor_size.width;
  let bar_left = bar_position.x;
  let bar_top = bar_position.y;
  let bar_right = bar_left + bar_size.width;
  let bar_bottom = bar_top + bar_size.height;
  let expanded_width = SELECTOR_EXPANDED_WIDTH;
  let expanded_height = SELECTOR_EXPANDED_HEIGHT;
  let collapsed_width = SELECTOR_COLLAPSED_WIDTH;
  let collapsed_height = SELECTOR_COLLAPSED_HEIGHT;
  let gap = SELECTOR_GAP;
  let available_above = bar_top - monitor_position.y;
  let placement = if available_above >= expanded_height + gap {
    SelectorPlacement::Above
  } else {
    SelectorPlacement::Below
  };
  let center_x = (bar_left + bar_right) / 2.0;
  let expanded_x =
    (center_x - expanded_width / 2.0).clamp(monitor_position.x, monitor_right - expanded_width);
  let collapsed_x =
    (center_x - collapsed_width / 2.0).clamp(monitor_position.x, monitor_right - collapsed_width);
  let (collapsed_y, expanded_y) = match placement {
    SelectorPlacement::Above => (
      bar_top - gap - collapsed_height,
      bar_top - gap - expanded_height,
    ),
    SelectorPlacement::Below => (bar_bottom + gap, bar_bottom + gap),
  };

  Ok((
    placement,
    SelectorFrame {
      position: LogicalPosition::new(collapsed_x, collapsed_y),
      size: LogicalSize::new(collapsed_width, collapsed_height),
    },
    SelectorFrame {
      position: LogicalPosition::new(expanded_x, expanded_y),
      size: LogicalSize::new(expanded_width, expanded_height),
    },
  ))
}

fn animate_selector(window: WebviewWindow, from: SelectorFrame, to: SelectorFrame) {
  let animation = SELECTOR_ANIMATION.fetch_add(1, Ordering::Relaxed) + 1;
  tauri::async_runtime::spawn_blocking(move || {
    for step in 1..=ANIMATION_STEPS {
      if SELECTOR_ANIMATION.load(Ordering::Relaxed) != animation {
        return;
      }

      let progress = step as f64 / ANIMATION_STEPS as f64;
      let eased = 1.0 - (1.0 - progress).powi(3);
      let interpolate = |start: f64, end: f64| start + (end - start) * eased;
      let position = LogicalPosition::new(
        interpolate(from.position.x, to.position.x),
        interpolate(from.position.y, to.position.y),
      );
      let size = LogicalSize::new(
        interpolate(from.size.width, to.size.width),
        interpolate(from.size.height, to.size.height),
      );

      let _ = window.set_position(position);
      let _ = window.set_size(size);
      std::thread::sleep(Duration::from_millis(10));
    }
  });
}

fn reposition_recording_source_selector(app: &AppHandle) -> tauri::Result<()> {
  let selector = app
    .get_webview_window(WindowLabel::RecordingSourceSelector.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  if !selector.is_visible()? {
    return Ok(());
  }

  let (placement, collapsed, expanded) = selector_frames(app)?;
  let target = if SELECTOR_EXPANDED.load(Ordering::Relaxed) {
    expanded
  } else {
    collapsed
  };
  SELECTOR_ANIMATION.fetch_add(1, Ordering::Relaxed);
  selector.set_size(target.size)?;
  selector.set_position(target.position)?;
  app.emit_to(
    WindowLabel::RecordingSourceSelector.as_str(),
    "recording-source-selector://placement",
    placement,
  )?;

  Ok(())
}

fn contain_recording_bar(app: &AppHandle) -> tauri::Result<()> {
  let bar = app
    .get_webview_window(WindowLabel::RecordingBar.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  let bar_position = bar.outer_position()?;
  let bar_size = bar.outer_size()?;
  let monitors = app.available_monitors()?;
  let target = monitors
    .iter()
    .max_by_key(|monitor| {
      let position = monitor.position();
      let size = monitor.size();
      let left = bar_position.x.max(position.x);
      let top = bar_position.y.max(position.y);
      let right = (bar_position.x + bar_size.width as i32).min(position.x + size.width as i32);
      let bottom = (bar_position.y + bar_size.height as i32).min(position.y + size.height as i32);
      i64::from((right - left).max(0)) * i64::from((bottom - top).max(0))
    })
    .or_else(|| monitors.first())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  let monitor_position = target.position();
  let monitor_size = target.size();
  let max_x = monitor_position.x + monitor_size.width.saturating_sub(bar_size.width) as i32;
  let max_y = monitor_position.y + monitor_size.height.saturating_sub(bar_size.height) as i32;
  let contained = PhysicalPosition::new(
    bar_position.x.clamp(monitor_position.x, max_x),
    bar_position.y.clamp(monitor_position.y, max_y),
  );

  if contained != bar_position {
    bar.set_position(contained)?;
  }

  Ok(())
}

#[tauri::command]
pub fn toggle_recording_source_selector(app: AppHandle) -> tauri::Result<()> {
  let window = app
    .get_webview_window(WindowLabel::RecordingSourceSelector.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  if SELECTOR_EXPANDED.load(Ordering::Relaxed) {
    return collapse_recording_source_selector(app);
  }
  let (placement, collapsed, expanded) = selector_frames(&app)?;

  if !window.is_visible()? {
    window.set_size(collapsed.size)?;
    window.set_position(collapsed.position)?;
    window.show()?;
  }
  SELECTOR_EXPANDED.store(true, Ordering::Relaxed);
  app.emit_to(
    WindowLabel::RecordingSourceSelector.as_str(),
    "recording-source-selector://expanded",
    placement,
  )?;
  animate_selector(window, collapsed, expanded);

  Ok(())
}

pub fn manage_recording_bar_movement(app: &AppHandle) {
  let Some(window) = app.get_webview_window(WindowLabel::RecordingBar.as_str()) else {
    return;
  };
  let app = app.clone();

  window.on_window_event(move |event| {
    if !matches!(event, WindowEvent::Moved(_)) {
      return;
    }

    let _ = reposition_recording_source_selector(&app);
    let settle = BAR_MOVE_SETTLE.fetch_add(1, Ordering::Relaxed) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
      std::thread::sleep(BAR_DRAG_SETTLE_DELAY);
      if BAR_MOVE_SETTLE.load(Ordering::Relaxed) != settle {
        return;
      }

      let _ = contain_recording_bar(&app);
      let _ = reposition_recording_source_selector(&app);
      let _ = app.save_window_state(StateFlags::POSITION);
    });
  });
}

#[tauri::command]
pub fn collapse_recording_source_selector(app: AppHandle) -> tauri::Result<()> {
  let window = app
    .get_webview_window(WindowLabel::RecordingSourceSelector.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  if !window.is_visible()? || !SELECTOR_EXPANDED.swap(false, Ordering::Relaxed) {
    return Ok(());
  }

  let (_, collapsed, _) = selector_frames(&app)?;
  let scale = window.scale_factor()?;
  let current = SelectorFrame {
    position: window.outer_position()?.to_logical(scale),
    size: window.outer_size()?.to_logical(scale),
  };
  app.emit_to(
    WindowLabel::RecordingSourceSelector.as_str(),
    "recording-source-selector://collapsed",
    (),
  )?;
  animate_selector(window, current, collapsed);

  Ok(())
}

#[tauri::command]
pub fn hide_recording_ui(app: AppHandle) -> tauri::Result<()> {
  SELECTOR_ANIMATION.fetch_add(1, Ordering::Relaxed);
  SELECTOR_EXPANDED.store(false, Ordering::Relaxed);
  if let Some(selector) = app.get_webview_window(WindowLabel::RecordingSourceSelector.as_str()) {
    selector.hide()?;
  }
  if let Some(bar) = app.get_webview_window(WindowLabel::RecordingBar.as_str()) {
    bar.hide()?;
  }

  Ok(())
}

pub fn show_recording_ui(app: &AppHandle) -> tauri::Result<()> {
  let bar = app
    .get_webview_window(WindowLabel::RecordingBar.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  show(&bar, false)?;

  let selector = app
    .get_webview_window(WindowLabel::RecordingSourceSelector.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  let (placement, collapsed, _) = selector_frames(app)?;
  let positioning = SELECTOR_ANIMATION.fetch_add(1, Ordering::Relaxed) + 1;
  SELECTOR_EXPANDED.store(false, Ordering::Relaxed);
  selector.set_size(collapsed.size)?;
  selector.set_position(collapsed.position)?;
  selector.show()?;
  app.emit_to(
    WindowLabel::RecordingSourceSelector.as_str(),
    "recording-source-selector://collapsed",
    placement,
  )?;

  // A hidden window can still report the scale factor of the monitor it was
  // last on. Reapply after AppKit has moved it to the bar's monitor so mixed-
  // DPI launches settle at the exact logical position.
  let app = app.clone();
  tauri::async_runtime::spawn_blocking(move || {
    std::thread::sleep(Duration::from_millis(75));
    if SELECTOR_ANIMATION.load(Ordering::Relaxed) != positioning {
      return;
    }
    let Some(selector) = app.get_webview_window(WindowLabel::RecordingSourceSelector.as_str())
    else {
      return;
    };
    if let Ok((_, collapsed, _)) = selector_frames(&app) {
      let _ = selector.set_size(collapsed.size);
      let _ = selector.set_position(collapsed.position);
    }
  });

  Ok(())
}

pub fn hide_instead_of_close(app: &AppHandle, label: WindowLabel) {
  if let Some(window) = app.get_webview_window(label.as_str()) {
    let window_to_hide = window.clone();
    window.on_window_event(move |event| {
      if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = window_to_hide.hide();
      }
    });
  }
}
