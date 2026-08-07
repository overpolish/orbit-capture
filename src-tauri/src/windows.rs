use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;
use tauri::{
  ipc::{Channel, InvokeResponseBody},
  AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, PhysicalPosition, PhysicalSize,
  WebviewWindow, WindowEvent,
};
use tauri_plugin_window_state::{AppHandleExt, StateFlags, WindowExt};

mod platform;

#[derive(Clone, Copy)]
pub enum WindowLabel {
  #[cfg(target_os = "macos")]
  Permissions,
  RecordingBar,
  RecordingOptions,
  RegionSelector,
  RecordingSourceSelector,
  StandaloneListbox,
}

impl WindowLabel {
  pub const fn as_str(self) -> &'static str {
    match self {
      #[cfg(target_os = "macos")]
      Self::Permissions => "permissions",
      Self::RecordingBar => "recording-bar",
      Self::RecordingOptions => "recording-options",
      Self::RegionSelector => "region-selector",
      Self::RecordingSourceSelector => "recording-source-selector",
      Self::StandaloneListbox => "standalone-listbox",
    }
  }
}

#[cfg(target_os = "macos")]
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

pub fn initialize_region_selector(app: &AppHandle) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(WindowLabel::RegionSelector.as_str()) {
    platform::initialize_region_selector(&window)?;
    window.set_ignore_cursor_events(true)?;
  }

  Ok(())
}

pub fn initialize_recording_options(app: &AppHandle) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(WindowLabel::RecordingOptions.as_str()) {
    platform::initialize_recording_options(&window)?;
  }

  Ok(())
}

pub fn initialize_standalone_listbox(app: &AppHandle) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(WindowLabel::StandaloneListbox.as_str()) {
    platform::initialize_standalone_listbox(&window)?;
  }

  Ok(())
}

const SELECTOR_COLLAPSED_WIDTH: f64 = 300.0;
const SELECTOR_COLLAPSED_HEIGHT: f64 = 40.0;
const SELECTOR_EXPANDED_WIDTH: f64 = 500.0;
const SELECTOR_EXPANDED_HEIGHT: f64 = 250.0;
const WINDOW_SELECTOR_EXPANDED_WIDTH: f64 = 750.0;
const WINDOW_SELECTOR_EXPANDED_HEIGHT: f64 = 500.0;
const SELECTOR_GAP: f64 = 6.0;
const RECORDING_OPTIONS_WIDTH: f64 = 240.0;
const RECORDING_OPTIONS_HEIGHT: f64 = 270.0;
const RECORDING_OPTIONS_GAP: f64 = 6.0;
const ANIMATION_STEPS: u64 = 18;
static SELECTOR_ANIMATION: AtomicU64 = AtomicU64::new(0);
static SELECTOR_EXPANDED: AtomicBool = AtomicBool::new(false);
static SELECTOR_VISIBLE: AtomicBool = AtomicBool::new(true);
static RECORDING_CONTROLS_VISIBLE: AtomicBool = AtomicBool::new(true);
static WINDOW_SELECTOR_ACTIVE: AtomicBool = AtomicBool::new(false);
static RECORDING_OPTIONS_VISIBLE: AtomicBool = AtomicBool::new(false);
static STANDALONE_LISTBOX_VISIBLE: AtomicBool = AtomicBool::new(false);
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

fn recording_options_frame(app: &AppHandle, anchor_x: f64) -> tauri::Result<LogicalPosition<f64>> {
  let bar = app
    .get_webview_window(WindowLabel::RecordingBar.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  let monitor = bar
    .current_monitor()?
    .or(app.primary_monitor()?)
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  let scale = monitor.scale_factor();
  let monitor_position = monitor.position().to_logical::<f64>(scale);
  let monitor_size = monitor.size().to_logical::<f64>(scale);
  let bar_position = bar.outer_position()?.to_logical::<f64>(scale);
  let bar_size = bar.outer_size()?.to_logical::<f64>(scale);
  let monitor_right = monitor_position.x + monitor_size.width;
  let monitor_bottom = monitor_position.y + monitor_size.height;
  let x = (bar_position.x + anchor_x - RECORDING_OPTIONS_WIDTH / 2.0)
    .clamp(monitor_position.x, monitor_right - RECORDING_OPTIONS_WIDTH);
  let available_above = bar_position.y - monitor_position.y;
  let y = if available_above >= RECORDING_OPTIONS_HEIGHT + RECORDING_OPTIONS_GAP {
    bar_position.y - RECORDING_OPTIONS_HEIGHT - RECORDING_OPTIONS_GAP
  } else {
    (bar_position.y + bar_size.height + RECORDING_OPTIONS_GAP)
      .min(monitor_bottom - RECORDING_OPTIONS_HEIGHT)
  };

  Ok(LogicalPosition::new(x, y))
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
pub fn toggle_recording_source_selector(
  app: AppHandle,
  window_selector: bool,
) -> tauri::Result<()> {
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

#[tauri::command]
pub fn toggle_recording_options(app: AppHandle, anchor_x: f64) -> tauri::Result<()> {
  if RECORDING_OPTIONS_VISIBLE.load(Ordering::Relaxed) {
    return hide_recording_options(app);
  }

  let window = app
    .get_webview_window(WindowLabel::RecordingOptions.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  window.set_size(LogicalSize::new(
    RECORDING_OPTIONS_WIDTH,
    RECORDING_OPTIONS_HEIGHT,
  ))?;
  window.set_position(recording_options_frame(&app, anchor_x)?)?;
  RECORDING_OPTIONS_VISIBLE.store(true, Ordering::Relaxed);
  platform::show(&window)?;
  platform::restore_recording_level(&window)?;
  app.emit_to(
    WindowLabel::RecordingOptions.as_str(),
    "recording-options://opened",
    (),
  )
}

#[tauri::command]
pub fn hide_recording_options(app: AppHandle) -> tauri::Result<()> {
  RECORDING_OPTIONS_VISIBLE.store(false, Ordering::Relaxed);
  hide_standalone_listbox(app.clone())?;
  crate::audio_preview::stop_all(&app);
  crate::camera_preview::stop_all(&app);
  if let Some(window) = app.get_webview_window(WindowLabel::RecordingOptions.as_str()) {
    platform::hide(&window)?;
  }
  app.emit_to(
    WindowLabel::RecordingOptions.as_str(),
    "recording-options://closed",
    (),
  )
}

#[tauri::command]
pub fn show_standalone_listbox(
  app: AppHandle,
  parent_window_label: String,
  offset: LogicalPosition<f64>,
  size: LogicalSize<f64>,
) -> tauri::Result<()> {
  let parent = app
    .get_webview_window(&parent_window_label)
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  let window = app
    .get_webview_window(WindowLabel::StandaloneListbox.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  let scale = parent.scale_factor()?;
  let parent_position = parent.outer_position()?.to_logical::<f64>(scale);
  let mut position =
    LogicalPosition::new(parent_position.x + offset.x, parent_position.y + offset.y);

  if let Some(monitor) = parent.current_monitor()?.or(app.primary_monitor()?) {
    let monitor_scale = monitor.scale_factor();
    let monitor_position = monitor.position().to_logical::<f64>(monitor_scale);
    let monitor_size = monitor.size().to_logical::<f64>(monitor_scale);
    let max_x = monitor_position.x + (monitor_size.width - size.width).max(0.0);
    let max_y = monitor_position.y + (monitor_size.height - size.height).max(0.0);
    position.x = position.x.clamp(monitor_position.x, max_x);
    position.y = position.y.clamp(monitor_position.y, max_y);
  }

  window.set_size(size)?;
  window.set_position(position)?;
  STANDALONE_LISTBOX_VISIBLE.store(true, Ordering::Relaxed);
  platform::show(&window)?;
  platform::restore_recording_level(&window)
}

#[tauri::command]
pub fn hide_standalone_listbox(app: AppHandle) -> tauri::Result<()> {
  STANDALONE_LISTBOX_VISIBLE.store(false, Ordering::Relaxed);
  if let Some(window) = app.get_webview_window(WindowLabel::StandaloneListbox.as_str()) {
    platform::hide(&window)?;
  }
  app.emit("standalone-listbox://closed", ())?;

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
            if !coordinate_is_in_window(x, y, &selector) {
              let _ = collapse_recording_source_selector(app.clone());
            }
          }
        }
        dismiss_recording_options_if_outside(&app, x, y);
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
          if !coordinate_is_in_window(position.x, position.y, &selector) {
            let _ = collapse_recording_source_selector(app.clone());
          }
        }
        dismiss_recording_options_if_outside(&app, position.x, position.y);
      }

      was_pressed = is_pressed;
      std::thread::sleep(Duration::from_millis(8));
    }
  });
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn manage_recording_source_selector_dismissal(_app: &AppHandle) {}

fn coordinate_is_in_window(x: f64, y: f64, window: &WebviewWindow) -> bool {
  let Ok(position) = window.outer_position() else {
    return false;
  };
  let Ok(size) = window.outer_size() else {
    return false;
  };
  let Ok(scale) = window.scale_factor() else {
    return false;
  };
  let position = position.to_logical::<f64>(scale);
  let size = size.to_logical::<f64>(scale);

  x >= position.x
    && x <= position.x + size.width
    && y >= position.y
    && y <= position.y + size.height
}

fn dismiss_recording_options_if_outside(app: &AppHandle, x: f64, y: f64) {
  if !RECORDING_OPTIONS_VISIBLE.load(Ordering::Relaxed) {
    return;
  }

  let is_in = |label: WindowLabel| {
    app
      .get_webview_window(label.as_str())
      .is_some_and(|window| coordinate_is_in_window(x, y, &window))
  };
  let inside_options = is_in(WindowLabel::RecordingOptions);
  let inside_listbox =
    STANDALONE_LISTBOX_VISIBLE.load(Ordering::Relaxed) && is_in(WindowLabel::StandaloneListbox);
  let inside_bar = is_in(WindowLabel::RecordingBar);

  if !inside_options && !inside_listbox && !inside_bar {
    let _ = hide_recording_options(app.clone());
  }
}

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
pub fn show_region_selector(
  app: AppHandle,
  position: PhysicalPosition<i32>,
  size: PhysicalSize<u32>,
) -> tauri::Result<()> {
  let region = app
    .get_webview_window(WindowLabel::RegionSelector.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  region.set_size(size)?;
  region.set_position(position)?;
  platform::set_opacity(&region, 1.0)?;
  platform::show(&region)?;
  platform::restore_recording_level(&region)?;

  raise_recording_controls(&app)?;

  #[cfg(target_os = "macos")]
  tauri::async_runtime::spawn_blocking(move || {
    // AppKit completes showing a previously hidden panel asynchronously and
    // can order it above panels raised in the same run-loop turn.
    std::thread::sleep(Duration::from_millis(75));
    let ordering_app = app.clone();
    let _ = app.run_on_main_thread(move || {
      let Some(region) = ordering_app.get_webview_window(WindowLabel::RegionSelector.as_str())
      else {
        return;
      };
      let _ = platform::restore_recording_level(&region);
      let _ = raise_recording_controls(&ordering_app);
    });
  });

  Ok(())
}

#[tauri::command]
pub fn hide_region_selector(app: AppHandle) -> tauri::Result<()> {
  if let Some(region) = app.get_webview_window(WindowLabel::RegionSelector.as_str()) {
    region.hide()?;
  }
  set_recording_controls_opacity(app, 1.0)
}

fn raise_recording_controls(app: &AppHandle) -> tauri::Result<()> {
  if !RECORDING_CONTROLS_VISIBLE.load(Ordering::Relaxed) {
    return Ok(());
  }

  if let Some(bar) = app.get_webview_window(WindowLabel::RecordingBar.as_str()) {
    platform::raise_without_activation(&bar)?;
  }
  if SELECTOR_VISIBLE.load(Ordering::Relaxed) {
    if let Some(selector) = app.get_webview_window(WindowLabel::RecordingSourceSelector.as_str()) {
      platform::raise_without_activation(&selector)?;
    }
  }
  Ok(())
}

#[tauri::command]
pub fn set_region_selector_passthrough(app: AppHandle, passthrough: bool) -> tauri::Result<()> {
  let region = app
    .get_webview_window(WindowLabel::RegionSelector.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  region.set_ignore_cursor_events(passthrough)?;
  if passthrough {
    #[cfg(target_os = "macos")]
    platform::resign_key(&region)?;
  } else {
    region.set_focus()?;
  }

  if passthrough {
    raise_recording_controls(&app)?;
  }

  Ok(())
}

#[tauri::command]
pub fn set_region_selector_opacity(app: AppHandle, opacity: f64) -> tauri::Result<()> {
  let region = app
    .get_webview_window(WindowLabel::RegionSelector.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  platform::set_opacity(&region, opacity)
}

#[tauri::command]
pub fn set_recording_controls_opacity(app: AppHandle, opacity: f64) -> tauri::Result<()> {
  RECORDING_CONTROLS_VISIBLE.store(opacity > 0.0, Ordering::Relaxed);

  if let Some(bar) = app.get_webview_window(WindowLabel::RecordingBar.as_str()) {
    platform::set_opacity(&bar, opacity)?;
  }
  if SELECTOR_VISIBLE.load(Ordering::Relaxed) {
    if let Some(selector) = app.get_webview_window(WindowLabel::RecordingSourceSelector.as_str()) {
      platform::set_opacity(&selector, opacity)?;
    }
  }
  if opacity > 0.0 {
    raise_recording_controls(&app)?;
  } else {
    let _ = collapse_recording_source_selector(app.clone());
  }
  Ok(())
}

#[tauri::command]
pub async fn take_monitor_screenshot(monitor_id: u32, channel: Channel) -> Result<(), String> {
  let screenshot = tauri::async_runtime::spawn_blocking(move || {
    let monitor = xcap::Monitor::all()
      .map_err(|error| error.to_string())?
      .into_iter()
      .find(|monitor| monitor.id().ok() == Some(monitor_id))
      .ok_or_else(|| "The selected monitor is no longer available".to_owned())?;
    monitor
      .capture_image()
      .map(|image| image.into_raw())
      .map_err(|error| error.to_string())
  })
  .await
  .map_err(|error| error.to_string())??;

  channel
    .send(InvokeResponseBody::Raw(screenshot))
    .map_err(|error| error.to_string())
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
  let bar = app
    .get_webview_window(WindowLabel::RecordingBar.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  show(&bar, false)?;
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
        if matches!(label, WindowLabel::RecordingOptions) {
          let _ = hide_recording_options(app.clone());
        } else {
          let _ = window_to_hide.hide();
        }
      }
    });
  }
}
