// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;
use tauri::{
  AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, PhysicalPosition, WebviewWindow,
  WindowEvent,
};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

pub(crate) mod dock;
mod geometry;
mod lifecycle;
pub(crate) mod monitor_capture;
pub(crate) mod options;
mod platform;
pub(crate) mod region;

pub use dock::{
  hide_recording_dock, initialize_recording_dock, manage_recording_dock_movement,
  show_recording_dock,
};
use geometry::monitor_with_most_overlap;
pub use lifecycle::{
  get_or_create, initialize_export, initialize_recording_bar, initialize_recording_bar_position,
  initialize_recording_options, initialize_recording_source_selector, initialize_region_selector,
  initialize_standalone_listbox, raise_export, show,
};
pub use options::hide_recording_options;
pub use region::{
  hide_region_selector, is_region_selector_visible, set_region_selector_passthrough,
};

#[derive(Clone, Copy)]
pub enum WindowLabel {
  Export,
  #[cfg(target_os = "macos")]
  Permissions,
  RecordingBar,
  RecordingDock,
  RecordingOptions,
  RegionSelector,
  RecordingSourceSelector,
  StandaloneListbox,
}

impl WindowLabel {
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Export => "export",
      #[cfg(target_os = "macos")]
      Self::Permissions => "permissions",
      Self::RecordingBar => "recording-bar",
      Self::RecordingDock => "recording-dock",
      Self::RecordingOptions => "recording-options",
      Self::RegionSelector => "region-selector",
      Self::RecordingSourceSelector => "recording-source-selector",
      Self::StandaloneListbox => "standalone-listbox",
    }
  }
}

// This is where a list of capture-excluded window labels used to live. Capture
// now excludes every window this process owns, matched on the owning process
// rather than by name, so a window added later is excluded the day it is added
// and there is no list left to forget to update. See `capture_kit::our_windows`.

const SELECTOR_COLLAPSED_WIDTH: f64 = 300.0;
const SELECTOR_COLLAPSED_HEIGHT: f64 = 40.0;
const SELECTOR_EXPANDED_WIDTH: f64 = 500.0;
const SELECTOR_EXPANDED_HEIGHT: f64 = 250.0;
const WINDOW_SELECTOR_EXPANDED_WIDTH: f64 = 750.0;
const WINDOW_SELECTOR_EXPANDED_HEIGHT: f64 = 500.0;
const SELECTOR_GAP: f64 = 6.0;
const ANIMATION_STEPS: u64 = 18;
static SELECTOR_ANIMATION: AtomicU64 = AtomicU64::new(0);
static SELECTOR_EXPANDED: AtomicBool = AtomicBool::new(false);
static SELECTOR_VISIBLE: AtomicBool = AtomicBool::new(true);
static RECORDING_CONTROLS_VISIBLE: AtomicBool = AtomicBool::new(true);
static WINDOW_SELECTOR_ACTIVE: AtomicBool = AtomicBool::new(false);
static REGION_SELECTOR_EDITING: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "windows")]
static BAR_DRAG_ACTIVE: AtomicBool = AtomicBool::new(false);

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
  #[cfg(target_os = "windows")]
  let bar_position = bar.inner_position()?;
  #[cfg(not(target_os = "windows"))]
  let bar_position = bar.outer_position()?;
  #[cfg(target_os = "windows")]
  let bar_size = bar.inner_size()?;
  #[cfg(not(target_os = "windows"))]
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
  #[cfg(target_os = "windows")]
  let selector_frame_offset = {
    let selector = app
      .get_webview_window(WindowLabel::RecordingSourceSelector.as_str())
      .ok_or_else(|| tauri::Error::WindowNotFound)?;
    let inner = selector.inner_position()?.to_logical::<f64>(scale);
    let outer = selector.outer_position()?.to_logical::<f64>(scale);
    LogicalPosition::new(inner.x - outer.x, inner.y - outer.y)
  };
  #[cfg(not(target_os = "windows"))]
  let selector_frame_offset = LogicalPosition::new(0.0, 0.0);
  let monitor_right = monitor_position.x + monitor_size.width;
  let bar_left = bar_position.x;
  let bar_top = bar_position.y;
  let bar_right = bar_left + bar_size.width;
  let bar_bottom = bar_top + bar_size.height;
  let (expanded_width, expanded_height) = if WINDOW_SELECTOR_ACTIVE.load(Ordering::Relaxed) {
    (
      WINDOW_SELECTOR_EXPANDED_WIDTH,
      WINDOW_SELECTOR_EXPANDED_HEIGHT,
    )
  } else {
    (SELECTOR_EXPANDED_WIDTH, SELECTOR_EXPANDED_HEIGHT)
  };
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
      position: LogicalPosition::new(
        collapsed_x - selector_frame_offset.x,
        collapsed_y - selector_frame_offset.y,
      ),
      size: LogicalSize::new(collapsed_width, collapsed_height),
    },
    SelectorFrame {
      position: LogicalPosition::new(
        expanded_x - selector_frame_offset.x,
        expanded_y - selector_frame_offset.y,
      ),
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
  let target = monitor_with_most_overlap(app, &bar)?.ok_or_else(|| tauri::Error::WindowNotFound)?;
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
pub fn toggle_recording_source_selector(
  app: AppHandle,
  window_selector: bool,
) -> tauri::Result<()> {
  // A recording hides this chrome deliberately; nothing may bring it back
  // until the recording is over.
  if !crate::recording::is_idle(&app) {
    return Ok(());
  }

  let window = app
    .get_webview_window(WindowLabel::RecordingSourceSelector.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  if SELECTOR_EXPANDED.load(Ordering::Relaxed) {
    return collapse_recording_source_selector(app);
  }
  WINDOW_SELECTOR_ACTIVE.store(window_selector, Ordering::Relaxed);
  let (placement, collapsed, expanded) = selector_frames(&app)?;

  if !window.is_visible()? {
    window.set_size(collapsed.size)?;
    window.set_position(collapsed.position)?;
    platform::show(&window)?;
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

pub fn hide_recording_bar(app: &AppHandle) -> tauri::Result<()> {
  // Clearing this first matters: anything that raises the recording controls
  // afterwards - hiding the region overlay, for instance - would otherwise
  // order the bar straight back on screen.
  RECORDING_CONTROLS_VISIBLE.store(false, Ordering::Relaxed);
  if let Some(bar) = app.get_webview_window(WindowLabel::RecordingBar.as_str()) {
    bar.hide()?;
  }

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
    let _ = hide_recording_options(app.clone());

    #[cfg(target_os = "windows")]
    watch_for_recording_bar_mouse_up(app.clone());
  });
}

#[cfg(target_os = "windows")]
pub fn manage_recording_source_selector_dismissal(app: &AppHandle) {
  use std::sync::{Arc, Mutex};

  use rdev::{listen, Button, EventType};

  let app = app.clone();
  let mouse_position = Arc::new(Mutex::new((0.0, 0.0)));
  std::thread::spawn(move || {
    let position = mouse_position.clone();
    let result = listen(move |event| match event.event_type {
      EventType::MouseMove { x, y } => {
        if let Ok(mut position) = position.lock() {
          *position = (x, y);
        }
      }
      EventType::ButtonRelease(Button::Left) => {
        let Ok((x, y)) = position.lock().map(|position| *position) else {
          return;
        };
        if SELECTOR_EXPANDED.load(Ordering::Relaxed) {
          if let Some(selector) =
            app.get_webview_window(WindowLabel::RecordingSourceSelector.as_str())
          {
            if !options::coordinate_is_in_window(x, y, &selector) {
              let _ = collapse_recording_source_selector(app.clone());
            }
          }
        }
        options::dismiss_if_outside(&app, x, y);
      }
      _ => {}
    });

    if let Err(error) = result {
      eprintln!("Could not monitor clicks for source selector dismissal: {error:?}");
    }
  });
}

#[cfg(target_os = "macos")]
pub fn manage_recording_source_selector_dismissal(app: &AppHandle) {
  use cidre::cg::{Event, EventSrcState, MouseButton};

  let app = app.clone();
  std::thread::spawn(move || {
    let mut was_pressed = EventSrcState::CombinedSession.button_state(MouseButton::Left);

    loop {
      let is_pressed = EventSrcState::CombinedSession.button_state(MouseButton::Left);
      if was_pressed && !is_pressed {
        let Some(event) = Event::with_src(None) else {
          break;
        };
        let position = event.location();
        if SELECTOR_EXPANDED.load(Ordering::Relaxed) {
          let Some(selector) =
            app.get_webview_window(WindowLabel::RecordingSourceSelector.as_str())
          else {
            break;
          };
          if !options::coordinate_is_in_window(position.x, position.y, &selector) {
            let _ = collapse_recording_source_selector(app.clone());
          }
        }
        options::dismiss_if_outside(&app, position.x, position.y);
      }

      was_pressed = is_pressed;
      std::thread::sleep(Duration::from_millis(8));
    }
  });
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn manage_recording_source_selector_dismissal(_app: &AppHandle) {}

#[cfg(target_os = "windows")]
fn watch_for_recording_bar_mouse_up(app: AppHandle) {
  use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};

  if BAR_DRAG_ACTIVE.swap(true, Ordering::Relaxed) {
    return;
  }

  tauri::async_runtime::spawn_blocking(move || {
    loop {
      let is_pressed = unsafe { GetAsyncKeyState(VK_LBUTTON.0.into()) } < 0;
      if !is_pressed {
        break;
      }
      std::thread::sleep(Duration::from_millis(8));
    }

    let _ = finish_recording_bar_drag(app);
    BAR_DRAG_ACTIVE.store(false, Ordering::Relaxed);
  });
}

#[tauri::command]
pub fn finish_recording_bar_drag(app: AppHandle) -> Result<(), String> {
  contain_recording_bar(&app).map_err(|error| error.to_string())?;
  reposition_recording_source_selector(&app).map_err(|error| error.to_string())?;
  app
    .save_window_state(StateFlags::POSITION)
    .map_err(|error| error.to_string())?;
  Ok(())
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

fn show_recording_source_selector(app: &AppHandle) -> tauri::Result<()> {
  let selector = app
    .get_webview_window(WindowLabel::RecordingSourceSelector.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  let (placement, collapsed, _) = selector_frames(app)?;
  #[cfg(target_os = "macos")]
  let positioning = SELECTOR_ANIMATION.fetch_add(1, Ordering::Relaxed) + 1;
  #[cfg(not(target_os = "macos"))]
  SELECTOR_ANIMATION.fetch_add(1, Ordering::Relaxed);
  SELECTOR_EXPANDED.store(false, Ordering::Relaxed);
  selector.set_size(collapsed.size)?;
  selector.set_position(collapsed.position)?;
  platform::show(&selector)?;
  platform::restore_recording_level(&selector)?;
  app.emit_to(
    WindowLabel::RecordingSourceSelector.as_str(),
    "recording-source-selector://collapsed",
    placement,
  )?;

  #[cfg(target_os = "macos")]
  let app = app.clone();
  #[cfg(target_os = "macos")]
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

#[tauri::command]
pub fn set_recording_source_selector_visible(app: AppHandle, visible: bool) -> tauri::Result<()> {
  SELECTOR_VISIBLE.store(visible, Ordering::Relaxed);
  if visible {
    if RECORDING_CONTROLS_VISIBLE.load(Ordering::Relaxed) {
      show_recording_source_selector(&app)
    } else {
      Ok(())
    }
  } else {
    SELECTOR_ANIMATION.fetch_add(1, Ordering::Relaxed);
    SELECTOR_EXPANDED.store(false, Ordering::Relaxed);
    if let Some(selector) = app.get_webview_window(WindowLabel::RecordingSourceSelector.as_str()) {
      platform::hide(&selector)?;
    }
    Ok(())
  }
}

#[tauri::command]
pub fn hide_recording_ui(app: AppHandle) -> tauri::Result<()> {
  SELECTOR_ANIMATION.fetch_add(1, Ordering::Relaxed);
  SELECTOR_EXPANDED.store(false, Ordering::Relaxed);
  hide_recording_options(app.clone())?;
  if let Some(selector) = app.get_webview_window(WindowLabel::RecordingSourceSelector.as_str()) {
    selector.hide()?;
  }
  if let Some(bar) = app.get_webview_window(WindowLabel::RecordingBar.as_str()) {
    bar.hide()?;
  }
  if let Some(region) = app.get_webview_window(WindowLabel::RegionSelector.as_str()) {
    region.hide()?;
  }

  Ok(())
}

pub fn show_recording_ui(app: &AppHandle) -> tauri::Result<()> {
  if !crate::recording::is_idle(app) {
    return Ok(());
  }

  RECORDING_CONTROLS_VISIBLE.store(true, Ordering::Relaxed);
  let bar = app
    .get_webview_window(WindowLabel::RecordingBar.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  show(&bar, false)?;
  // Asserted rather than assumed: the bar may have been faded out for region
  // editing, and requests to fade it back in are refused while a recording is
  // on. Coming back to idle is where that is put right.
  platform::set_opacity(&bar, 1.0)?;
  platform::restore_recording_level(&bar)?;

  if SELECTOR_VISIBLE.load(Ordering::Relaxed) {
    show_recording_source_selector(app)?;
  }
  app.emit_to(
    WindowLabel::RecordingBar.as_str(),
    "recording-ui://shown",
    (),
  )
}

pub fn hide_instead_of_close(app: &AppHandle, label: WindowLabel) {
  if let Some(window) = app.get_webview_window(label.as_str()) {
    let app = app.clone();
    let window_to_hide = window.clone();
    window.on_window_event(move |event| {
      if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        match label {
          WindowLabel::RecordingOptions => {
            let _ = hide_recording_options(app.clone());
          }
          // Closing the export window is the same act as cancelling: the
          // pending capture goes with it rather than lingering unseen.
          WindowLabel::Export => crate::exports::discard(&app),
          _ => {
            let _ = window_to_hide.hide();
          }
        }
      }
    });
  }
}
