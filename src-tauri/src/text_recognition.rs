// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use serde::Serialize;
use std::sync::Mutex;

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::recording::Region;
use crate::screenshots::{self, ScreenshotTarget};

#[cfg(target_os = "macos")]
mod platform_macos;
#[cfg(target_os = "windows")]
mod platform_windows;

const WINDOW_PREFIX: &str = "text-recognition-";

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextRect {
  pub x: f64,
  pub y: f64,
  pub width: f64,
  pub height: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecognizedCharacter {
  pub start: usize,
  pub end: usize,
  pub bounds: TextRect,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecognizedLine {
  pub text: String,
  pub confidence: f32,
  pub bounds: TextRect,
  pub characters: Vec<RecognizedCharacter>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextRecognitionResult {
  pub lines: Vec<RecognizedLine>,
  pub text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedTextRegion {
  pub image_png: Vec<u8>,
  pub width: u32,
  pub height: u32,
}

#[derive(Default)]
pub struct TextRecognitionState(Mutex<Option<screenshots::CapturedImage>>);

fn recognition_windows(app: &AppHandle) -> Vec<tauri::WebviewWindow> {
  app
    .webview_windows()
    .into_values()
    .filter(|window| window.label().starts_with(WINDOW_PREFIX))
    .collect()
}

fn close_recognition_windows(app: &AppHandle, except: Option<&str>) {
  for window in recognition_windows(app) {
    if Some(window.label()) != except {
      let _ = window.close();
    }
  }
}

pub fn dismiss(app: &AppHandle) {
  close_recognition_windows(app, None);
  *app
    .state::<TextRecognitionState>()
    .0
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}

#[cfg(target_os = "windows")]
fn set_recognition_capture_protected(
  window: &tauri::WebviewWindow,
  protected: bool,
) -> Result<(), String> {
  window
    .set_content_protected(protected)
    .map_err(|error| error.to_string())?;
  unsafe { windows::Win32::Graphics::Dwm::DwmFlush() }.map_err(|error| error.to_string())
}

pub fn start(app: &AppHandle) -> Result<(), String> {
  dismiss(app);

  let capture_monitors = xcap::Monitor::all().map_err(|error| error.to_string())?;
  let tauri_monitors = app
    .available_monitors()
    .map_err(|error| error.to_string())?;
  if capture_monitors.len() != tauri_monitors.len() {
    return Err("Tauri and xcap returned different monitor counts".to_owned());
  }

  for (index, (capture_monitor, monitor)) in
    capture_monitors.into_iter().zip(tauri_monitors).enumerate()
  {
    let monitor_id = capture_monitor.id().map_err(|error| error.to_string())?;
    let scale = monitor.scale_factor();
    let position = monitor.position().to_logical::<f64>(scale);
    let size = monitor.size().to_logical::<f64>(scale);
    let label = format!("{WINDOW_PREFIX}{index}");
    let window = WebviewWindowBuilder::new(
      app,
      label,
      WebviewUrl::App(format!("/text-recognition?monitorId={monitor_id}").into()),
    )
    .accept_first_mouse(true)
    .always_on_top(true)
    .decorations(false)
    .focused(index == 0)
    .inner_size(size.width, size.height)
    .position(position.x, position.y)
    .resizable(false)
    .shadow(false)
    .skip_taskbar(true)
    .transparent(true)
    .visible(false)
    .visible_on_all_workspaces(true)
    .build()
    .map_err(|error| error.to_string())?;
    #[cfg(not(target_os = "windows"))]
    window
      .set_content_protected(true)
      .map_err(|error| error.to_string())?;
    crate::windows::show(&window, index == 0).map_err(|error| error.to_string())?;
  }

  Ok(())
}

#[tauri::command]
pub fn start_text_recognition(app: AppHandle) -> Result<(), String> {
  start(&app)
}

#[tauri::command]
pub fn cancel_text_recognition(app: AppHandle) {
  dismiss(&app);
}

#[tauri::command]
pub fn copy_recognized_text(app: AppHandle, text: String) -> Result<(), String> {
  app
    .clipboard()
    .write_text(text)
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn capture_text_region(
  app: AppHandle,
  window: tauri::WebviewWindow,
  state: tauri::State<'_, TextRecognitionState>,
  monitor_id: u32,
  region: Region,
) -> Result<CapturedTextRegion, String> {
  if region.size.width < 2.0 || region.size.height < 2.0 {
    return Err("Draw a larger area around the text".to_owned());
  }

  close_recognition_windows(&app, Some(window.label()));
  let excluded_window_ids = recognition_windows(&app)
    .iter()
    .filter_map(platform_window_id)
    .collect::<Vec<_>>();

  #[cfg(target_os = "windows")]
  set_recognition_capture_protected(&window, true)?;
  let image = screenshots::capture_for_text_recognition(
    &app,
    ScreenshotTarget::Region { monitor_id, region },
    &excluded_window_ids,
  )
  .await;
  #[cfg(target_os = "windows")]
  set_recognition_capture_protected(
    &window,
    !crate::settings::current(&app).record_orbit_windows,
  )?;
  let image = image?;
  let image_png = screenshots::encoding::encode_truecolor_png(&image)?;
  let result = CapturedTextRegion {
    height: image.height,
    image_png,
    width: image.width,
  };
  *state
    .0
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(image);
  Ok(result)
}

#[tauri::command]
pub async fn recognize_captured_text(
  state: tauri::State<'_, TextRecognitionState>,
) -> Result<TextRecognitionResult, String> {
  let image = state
    .0
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .clone()
    .ok_or_else(|| "The selected image is no longer available".to_owned())?;
  let lines = recognize(image.rgba, image.width, image.height).await?;
  let text = lines
    .iter()
    .map(|line| line.text.as_str())
    .collect::<Vec<_>>()
    .join("\n");
  Ok(TextRecognitionResult { lines, text })
}

async fn recognize(rgba: Vec<u8>, width: u32, height: u32) -> Result<Vec<RecognizedLine>, String> {
  tauri::async_runtime::spawn_blocking(move || {
    #[cfg(target_os = "macos")]
    return platform_macos::recognize(&rgba, width, height);

    #[cfg(target_os = "windows")]
    return platform_windows::recognize(&rgba, width, height);

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    Err("Text recognition is not available on this platform".to_owned())
  })
  .await
  .map_err(|error| error.to_string())?
}

#[cfg(target_os = "macos")]
fn platform_window_id(window: &tauri::WebviewWindow) -> Option<u32> {
  use objc2::msg_send;
  let ns_window = window.ns_window().ok()?;
  let number: isize =
    unsafe { msg_send![ns_window.cast::<objc2::runtime::AnyObject>(), windowNumber] };
  u32::try_from(number).ok()
}

#[cfg(not(target_os = "macos"))]
fn platform_window_id(_window: &tauri::WebviewWindow) -> Option<u32> {
  None
}
