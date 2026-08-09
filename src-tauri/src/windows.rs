use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{
  ipc::{Channel, InvokeResponseBody},
  AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Monitor, PhysicalPosition,
  PhysicalSize, WebviewWindow, WindowEvent,
};
use tauri_plugin_window_state::{AppHandleExt, StateFlags, WindowExt};

mod platform;

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

/// The area, in physical pixels, that a window shares with a monitor.
fn overlap_area(
  monitor_position: PhysicalPosition<i32>,
  monitor_size: PhysicalSize<u32>,
  window_position: PhysicalPosition<i32>,
  window_size: PhysicalSize<u32>,
) -> i64 {
  let left = window_position.x.max(monitor_position.x);
  let top = window_position.y.max(monitor_position.y);
  let right = (window_position.x + window_size.width as i32)
    .min(monitor_position.x + monitor_size.width as i32);
  let bottom = (window_position.y + window_size.height as i32)
    .min(monitor_position.y + monitor_size.height as i32);

  i64::from((right - left).max(0)) * i64::from((bottom - top).max(0))
}

/// Whether any part of a window still lands on a connected monitor. A saved
/// position stops being usable the moment its display is unplugged or moved.
fn window_is_on_a_monitor(app: &AppHandle, window: &WebviewWindow) -> tauri::Result<bool> {
  let window_position = window.outer_position()?;
  let window_size = window.outer_size()?;

  Ok(app.available_monitors()?.iter().any(|monitor| {
    overlap_area(
      *monitor.position(),
      *monitor.size(),
      window_position,
      window_size,
    ) > 0
  }))
}

/// The monitor a window sits on most, for containment purposes.
fn monitor_with_most_overlap(
  app: &AppHandle,
  window: &WebviewWindow,
) -> tauri::Result<Option<Monitor>> {
  let window_position = window.outer_position()?;
  let window_size = window.outer_size()?;
  let monitors = app.available_monitors()?;
  let target = monitors
    .iter()
    .max_by_key(|monitor| {
      overlap_area(
        *monitor.position(),
        *monitor.size(),
        window_position,
        window_size,
      )
    })
    .or_else(|| monitors.first());

  Ok(target.cloned())
}

fn keep_window_on_a_monitor(app: &AppHandle, window: &WebviewWindow) -> tauri::Result<()> {
  let window_size = window.outer_size()?;

  if !window_is_on_a_monitor(app, window)? {
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

const RECORDING_DOCK_POSITION_FILE: &str = "recording-dock-position.json";

/// The pill's position, held against the work area of the monitor it was
/// dropped on rather than as absolute desktop coordinates, so it lands in the
/// same visual spot whichever monitor the recording bar is on.
///
/// The offset is in *logical* pixels. Physical pixels would move the pill twice
/// as far from the corner when a 2x display's offset is applied to a 1x one,
/// and a proportional fraction would distort placement for a fixed-size window
/// that is meant to sit a fixed distance from an edge.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
struct RecordingDockOffset {
  x: f64,
  y: f64,
}

fn recording_dock_offset_path(app: &AppHandle) -> tauri::Result<PathBuf> {
  Ok(
    app
      .path()
      .app_config_dir()?
      .join(RECORDING_DOCK_POSITION_FILE),
  )
}

fn load_recording_dock_offset(app: &AppHandle) {
  let offset = recording_dock_offset_path(app)
    .ok()
    .and_then(|path| std::fs::read(path).ok())
    .and_then(|contents| serde_json::from_slice::<RecordingDockOffset>(&contents).ok());

  *RECORDING_DOCK_OFFSET
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner()) = offset;
}

fn store_recording_dock_offset(app: &AppHandle, offset: RecordingDockOffset) -> tauri::Result<()> {
  *RECORDING_DOCK_OFFSET
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(offset);

  let path = recording_dock_offset_path(app)?;
  if let Some(directory) = path.parent() {
    std::fs::create_dir_all(directory)?;
  }
  let contents = serde_json::to_vec_pretty(&offset).map_err(std::io::Error::other)?;
  std::fs::write(path, contents)?;

  Ok(())
}

pub fn initialize_recording_dock(app: &AppHandle) -> tauri::Result<()> {
  load_recording_dock_offset(app);
  if let Some(window) = app.get_webview_window(WindowLabel::RecordingDock.as_str()) {
    platform::initialize_recording_dock(&window)?;
  }

  Ok(())
}

pub fn initialize_export(window: &WebviewWindow) -> tauri::Result<()> {
  platform::initialize_export(window)
}

pub fn raise_export(window: &WebviewWindow) -> tauri::Result<()> {
  platform::raise_export(window)
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
const RECORDING_DOCK_WIDTH: f64 = 216.0;
const RECORDING_DOCK_HEIGHT: f64 = 44.0;
const RECORDING_DOCK_TOP_GAP: f64 = 8.0;
const ANIMATION_STEPS: u64 = 18;
static SELECTOR_ANIMATION: AtomicU64 = AtomicU64::new(0);
static SELECTOR_EXPANDED: AtomicBool = AtomicBool::new(false);
static SELECTOR_VISIBLE: AtomicBool = AtomicBool::new(true);
static RECORDING_CONTROLS_VISIBLE: AtomicBool = AtomicBool::new(true);
static WINDOW_SELECTOR_ACTIVE: AtomicBool = AtomicBool::new(false);
static RECORDING_OPTIONS_VISIBLE: AtomicBool = AtomicBool::new(false);
static REGION_SELECTOR_EDITING: AtomicBool = AtomicBool::new(false);
static STANDALONE_LISTBOX_VISIBLE: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "windows")]
static BAR_DRAG_ACTIVE: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "windows")]
static DOCK_DRAG_ACTIVE: AtomicBool = AtomicBool::new(false);
/// Where the user dropped the pill, relative to the work area of the monitor
/// they dropped it on. `None` means it has never been dragged.
static RECORDING_DOCK_OFFSET: Mutex<Option<RecordingDockOffset>> = Mutex::new(None);
/// The position the pill was last placed at programmatically, so that a plain
/// click on its buttons is never mistaken for a drag.
static RECORDING_DOCK_PLACED: Mutex<Option<PhysicalPosition<i32>>> = Mutex::new(None);

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

#[tauri::command]
pub fn toggle_recording_options(app: AppHandle, anchor_x: f64) -> tauri::Result<()> {
  if !crate::recording::is_idle(&app) {
    return Ok(());
  }

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

/// The pill follows the recording bar rather than the recorded screen: it is
/// excluded from capture, so it never has to sit on the target monitor, and
/// following the bar puts it where the user is already looking.
fn recording_dock_monitor(app: &AppHandle) -> tauri::Result<Option<Monitor>> {
  if let Some(bar) = app.get_webview_window(WindowLabel::RecordingBar.as_str()) {
    // Geometry rather than `current_monitor`, because the bar is already
    // hidden by the time the pill is shown.
    if let Some(monitor) = monitor_with_most_overlap(app, &bar)? {
      return Ok(Some(monitor));
    }
  }

  app.primary_monitor()
}

/// Places the pill inside a work area: at its saved offset when it has one,
/// otherwise top-centre with a small gap. Always clamped so it stays wholly
/// inside, which is what makes a saved offset survive a move to a smaller
/// monitor.
fn recording_dock_local_position(
  work_area_size: PhysicalSize<u32>,
  dock_size: PhysicalSize<u32>,
  scale: f64,
  offset: Option<RecordingDockOffset>,
) -> (i32, i32) {
  let max_x = f64::from(work_area_size.width.saturating_sub(dock_size.width));
  let max_y = f64::from(work_area_size.height.saturating_sub(dock_size.height));
  let (x, y) = match offset {
    // Offsets are stored in logical pixels, so a pill dropped 200pt from the
    // corner of a Retina display lands 200pt from the corner of a 1x one.
    Some(offset) => (offset.x * scale, offset.y * scale),
    None => (max_x / 2.0, RECORDING_DOCK_TOP_GAP * scale),
  };

  (
    x.clamp(0.0, max_x).round() as i32,
    y.clamp(0.0, max_y).round() as i32,
  )
}

fn recording_dock_position(
  monitor: &Monitor,
  offset: Option<RecordingDockOffset>,
) -> PhysicalPosition<i32> {
  let scale = monitor.scale_factor();
  let work_area = monitor.work_area();
  // Derived from the target monitor's scale rather than read back from the
  // window, whose physical size still belongs to the monitor it is leaving.
  let dock_size = PhysicalSize {
    width: (RECORDING_DOCK_WIDTH * scale).round() as u32,
    height: (RECORDING_DOCK_HEIGHT * scale).round() as u32,
  };
  let (x, y) = recording_dock_local_position(work_area.size, dock_size, scale, offset);

  PhysicalPosition {
    x: work_area.position.x + x,
    y: work_area.position.y + y,
  }
}

pub fn show_recording_dock(app: &AppHandle) -> tauri::Result<()> {
  let dock = app
    .get_webview_window(WindowLabel::RecordingDock.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  dock.set_size(LogicalSize::new(
    RECORDING_DOCK_WIDTH,
    RECORDING_DOCK_HEIGHT,
  ))?;

  if let Some(monitor) = recording_dock_monitor(app)? {
    let offset = *RECORDING_DOCK_OFFSET
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    let position = recording_dock_position(&monitor, offset);
    dock.set_position(position)?;
    *RECORDING_DOCK_PLACED
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(position);
  }

  platform::show(&dock)?;
  platform::restore_recording_level(&dock)
}

/// Clamps the pill into the work area it mostly sits on, so a drag cannot park
/// it under the menu bar, the notch or the taskbar.
fn contain_recording_dock(app: &AppHandle, dock: &WebviewWindow) -> tauri::Result<()> {
  let dock_position = dock.outer_position()?;
  let dock_size = dock.outer_size()?;
  let Some(monitor) = monitor_with_most_overlap(app, dock)? else {
    return Ok(());
  };
  let work_area = monitor.work_area();
  let max_x = work_area.position.x + work_area.size.width.saturating_sub(dock_size.width) as i32;
  let max_y = work_area.position.y + work_area.size.height.saturating_sub(dock_size.height) as i32;
  let contained = PhysicalPosition::new(
    dock_position
      .x
      .clamp(work_area.position.x, max_x.max(work_area.position.x)),
    dock_position
      .y
      .clamp(work_area.position.y, max_y.max(work_area.position.y)),
  );

  if contained != dock_position {
    dock.set_position(contained)?;
  }

  Ok(())
}

/// The pill's position expressed against the work area it was dropped on.
fn recording_dock_offset(
  app: &AppHandle,
  dock: &WebviewWindow,
) -> tauri::Result<Option<RecordingDockOffset>> {
  let Some(monitor) = monitor_with_most_overlap(app, dock)? else {
    return Ok(None);
  };
  let scale = monitor.scale_factor();
  let work_area = monitor.work_area();
  let dock_position = dock.outer_position()?;
  let dock_size = dock.outer_size()?;
  let max_x = f64::from(work_area.size.width.saturating_sub(dock_size.width));
  let max_y = f64::from(work_area.size.height.saturating_sub(dock_size.height));

  Ok(Some(RecordingDockOffset {
    x: f64::from(dock_position.x - work_area.position.x).clamp(0.0, max_x) / scale,
    y: f64::from(dock_position.y - work_area.position.y).clamp(0.0, max_y) / scale,
  }))
}

#[tauri::command]
pub fn finish_recording_dock_drag(app: AppHandle) -> Result<(), String> {
  let to_message = |error: tauri::Error| error.to_string();
  let dock = app
    .get_webview_window(WindowLabel::RecordingDock.as_str())
    .ok_or_else(|| "The recording pill is unavailable".to_owned())?;
  contain_recording_dock(&app, &dock).map_err(to_message)?;

  let position = dock.outer_position().map_err(to_message)?;
  let placed = *RECORDING_DOCK_PLACED
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());
  // Pointer-up also fires for a plain click on the pill's buttons. Persisting
  // then would turn "never dragged" into a saved offset, and the pill would
  // stop using the default placement on whichever monitor the bar is on.
  if placed == Some(position) {
    return Ok(());
  }

  *RECORDING_DOCK_PLACED
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(position);
  if let Some(offset) = recording_dock_offset(&app, &dock).map_err(to_message)? {
    store_recording_dock_offset(&app, offset).map_err(to_message)?;
  }

  Ok(())
}

/// Windows does not deliver a pointer-up to the webview after a native drag,
/// so the pill's position is committed the same way the bar's is.
#[cfg(target_os = "windows")]
pub fn manage_recording_dock_movement(app: &AppHandle) {
  let Some(window) = app.get_webview_window(WindowLabel::RecordingDock.as_str()) else {
    return;
  };
  let app = app.clone();

  window.on_window_event(move |event| {
    if !matches!(event, WindowEvent::Moved(_)) {
      return;
    }

    watch_for_recording_dock_mouse_up(app.clone());
  });
}

#[cfg(not(target_os = "windows"))]
pub fn manage_recording_dock_movement(_app: &AppHandle) {}

#[cfg(target_os = "windows")]
fn watch_for_recording_dock_mouse_up(app: AppHandle) {
  use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};

  if DOCK_DRAG_ACTIVE.swap(true, Ordering::Relaxed) {
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

    let _ = finish_recording_dock_drag(app);
    DOCK_DRAG_ACTIVE.store(false, Ordering::Relaxed);
  });
}

pub fn hide_recording_dock(app: &AppHandle) -> tauri::Result<()> {
  if let Some(dock) = app.get_webview_window(WindowLabel::RecordingDock.as_str()) {
    platform::hide(&dock)?;
  }

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

pub fn is_region_selector_visible(app: &AppHandle) -> bool {
  app
    .get_webview_window(WindowLabel::RegionSelector.as_str())
    .is_some_and(|region| region.is_visible().unwrap_or(false))
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
  apply_region_selector_interactivity(&app)?;

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

/// The region overlay may take clicks only while the user is actively editing
/// the region, and only outside a recording. Every other time it is on screen -
/// displaying the chosen frame, or standing in as the recording boundary - it
/// has to let clicks through to whatever is underneath.
const fn region_selector_is_interactive(is_editing: bool, is_recording_idle: bool) -> bool {
  is_editing && is_recording_idle
}

/// Re-asserts that invariant against the window.
///
/// This has to be called by everything that shows the overlay, because
/// `platform::show` turns cursor events back on every time it runs. Leaving it
/// to the caller to remember is what made the desktop stop accepting clicks
/// after a re-show.
fn apply_region_selector_interactivity(app: &AppHandle) -> tauri::Result<()> {
  let Some(region) = app.get_webview_window(WindowLabel::RegionSelector.as_str()) else {
    return Ok(());
  };
  let is_interactive = region_selector_is_interactive(
    REGION_SELECTOR_EDITING.load(Ordering::Relaxed),
    crate::recording::is_idle(app),
  );

  region.set_ignore_cursor_events(!is_interactive)?;
  if is_interactive {
    region.set_focus()?;
  } else {
    #[cfg(target_os = "macos")]
    platform::resign_key(&region)?;
    raise_recording_controls(app)?;
  }

  Ok(())
}

#[tauri::command]
pub fn set_region_selector_passthrough(app: AppHandle, passthrough: bool) -> tauri::Result<()> {
  REGION_SELECTOR_EDITING.store(!passthrough, Ordering::Relaxed);
  apply_region_selector_interactivity(&app)
}

#[tauri::command]
pub fn set_region_selector_opacity(app: AppHandle, opacity: f64) -> tauri::Result<()> {
  let region = app
    .get_webview_window(WindowLabel::RegionSelector.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  platform::set_opacity(&region, opacity)
}

/// Fades the recording controls, and is the only thing that decides they are
/// on screen.
///
/// Fading them *in* is refused outside an idle app. The controls belong to the
/// idle state; while a recording is starting or running they are deliberately
/// gone. Several callers ask for opacity 1.0 without knowing that - hiding the
/// region overlay does it as cleanup, and the overlay window does it whenever
/// it stops editing - and `prepare_windows` itself hides the bar and then
/// hides the region overlay, which used to put the bar straight back.
#[tauri::command]
pub fn set_recording_controls_opacity(app: AppHandle, opacity: f64) -> tauri::Result<()> {
  if opacity > 0.0 && !crate::recording::is_idle(&app) {
    return Ok(());
  }

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

#[cfg(test)]
mod tests {
  use super::*;

  const DOCK: PhysicalSize<u32> = PhysicalSize {
    width: 216,
    height: 44,
  };
  const WORK_AREA: PhysicalSize<u32> = PhysicalSize {
    width: 1440,
    height: 850,
  };

  #[test]
  fn the_region_overlay_takes_clicks_only_while_editing_outside_a_recording() {
    assert!(region_selector_is_interactive(true, true));
    assert!(!region_selector_is_interactive(false, true));
    assert!(!region_selector_is_interactive(true, false));
    assert!(!region_selector_is_interactive(false, false));
  }

  #[test]
  fn centres_the_pill_below_the_work_area_top_when_it_was_never_dragged() {
    let (x, y) = recording_dock_local_position(WORK_AREA, DOCK, 1.0, None);
    assert_eq!(x, (1440 - 216) / 2);
    assert_eq!(y, RECORDING_DOCK_TOP_GAP as i32);
  }

  #[test]
  fn scales_the_default_gap_with_the_monitor() {
    let work_area = PhysicalSize {
      width: 2880,
      height: 1700,
    };
    let dock = PhysicalSize {
      width: 432,
      height: 88,
    };
    let (x, y) = recording_dock_local_position(work_area, dock, 2.0, None);
    assert_eq!(x, (2880 - 432) / 2);
    assert_eq!(y, (RECORDING_DOCK_TOP_GAP * 2.0) as i32);
  }

  #[test]
  fn applies_a_saved_offset_relative_to_the_work_area() {
    let offset = Some(RecordingDockOffset { x: 200.0, y: 60.0 });
    let (x, y) = recording_dock_local_position(WORK_AREA, DOCK, 1.0, offset);
    assert_eq!((x, y), (200, 60));
  }

  #[test]
  fn keeps_a_saved_offset_the_same_visual_distance_on_a_retina_monitor() {
    // The same logical offset, applied to a 2x display, has to land twice as
    // far along in physical pixels to look identical.
    let offset = Some(RecordingDockOffset { x: 200.0, y: 60.0 });
    let work_area = PhysicalSize {
      width: 2880,
      height: 1700,
    };
    let dock = PhysicalSize {
      width: 432,
      height: 88,
    };
    let (x, y) = recording_dock_local_position(work_area, dock, 2.0, offset);
    assert_eq!((x, y), (400, 120));
  }

  #[test]
  fn clamps_a_saved_offset_onto_a_smaller_monitor() {
    let offset = Some(RecordingDockOffset {
      x: 2_000.0,
      y: 1_400.0,
    });
    let work_area = PhysicalSize {
      width: 1280,
      height: 700,
    };
    let (x, y) = recording_dock_local_position(work_area, DOCK, 1.0, offset);
    assert_eq!((x, y), (1280 - 216, 700 - 44));
  }

  #[test]
  fn clamps_a_negative_offset_back_inside_the_work_area() {
    let offset = Some(RecordingDockOffset { x: -50.0, y: -80.0 });
    let (x, y) = recording_dock_local_position(WORK_AREA, DOCK, 1.0, offset);
    assert_eq!((x, y), (0, 0));
  }

  #[test]
  fn keeps_a_pill_wider_than_its_work_area_at_the_origin() {
    let work_area = PhysicalSize {
      width: 100,
      height: 20,
    };
    let (x, y) = recording_dock_local_position(work_area, DOCK, 1.0, None);
    assert_eq!((x, y), (0, 0));
  }
}
