// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::recording::Region;
use crate::screenshots;
use crate::windows::WindowLabel;

#[cfg(target_os = "macos")]
mod platform_macos;
#[cfg(target_os = "windows")]
mod platform_windows;
pub(crate) mod snapshot;

pub use snapshot::TextRecognitionState;

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
      // Windows animates hiding/destruction of a visible top-level window.
      // Because the OCR surface spans the monitor, that transition makes the
      // blue text selection visibly slide and shrink. Clear the layered alpha
      // before either visibility operation so the compositor has no OCR
      // pixels left to animate. macOS keeps its established close path.
      #[cfg(target_os = "windows")]
      let _ = crate::windows::conceal_disposable_overlay(&window);
      let _ = window.close();
    }
  }
}

pub fn dismiss(app: &AppHandle) {
  let had_windows = !recognition_windows(app).is_empty();
  close_recognition_windows(app, None);
  let had_capture = app.state::<TextRecognitionState>().cancel();
  if had_windows || had_capture {
    let _ = app.emit_to(
      WindowLabel::RecordingBar.as_str(),
      "text-recognition://ended",
      (),
    );
  }
}

// xcap::Monitor wraps a raw display handle that is not Send on every
// platform, so the command future may never hold one across an await.
// Enumerating synchronously drops the handles before the first snapshot.
fn monitor_layout(app: &AppHandle) -> Result<Vec<(u32, f64, tauri::Monitor)>, String> {
  let capture_monitors = xcap::Monitor::all().map_err(|error| error.to_string())?;
  let tauri_monitors = app
    .available_monitors()
    .map_err(|error| error.to_string())?;
  if capture_monitors.len() != tauri_monitors.len() {
    return Err("Tauri and xcap returned different monitor counts".to_owned());
  }

  capture_monitors
    .into_iter()
    .zip(tauri_monitors)
    .map(|(capture_monitor, monitor)| {
      let monitor_id = capture_monitor.id().map_err(|error| error.to_string())?;
      let scale = monitor.scale_factor();
      Ok((monitor_id, scale, monitor))
    })
    .collect()
}

pub async fn start(app: &AppHandle) -> Result<(), String> {
  dismiss(app);
  let generation = app.state::<TextRecognitionState>().begin();

  let monitors = monitor_layout(app)?;
  let mut snapshots = Vec::with_capacity(monitors.len());
  for (monitor_id, scale, _) in &monitors {
    let image = screenshots::capture_text_recognition_snapshot(*monitor_id).await?;
    snapshots.push((*monitor_id, *scale, image));
  }
  if !app
    .state::<TextRecognitionState>()
    .install(generation, snapshots)
  {
    return Ok(());
  }

  for (index, (monitor_id, scale, monitor)) in monitors.into_iter().enumerate() {
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

  let _ = app.emit_to(
    WindowLabel::RecordingBar.as_str(),
    "text-recognition://started",
    (),
  );

  Ok(())
}

pub fn start_detached(app: &AppHandle) {
  let app = app.clone();
  tauri::async_runtime::spawn(async move {
    if let Err(error) = start(&app).await {
      eprintln!("Could not start text recognition: {error}");
    }
  });
}

#[tauri::command]
pub async fn start_text_recognition(app: AppHandle) -> Result<(), String> {
  start(&app).await
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
pub fn capture_text_region(
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
  let image = state.select_region(monitor_id, region)?;
  let image_png = screenshots::encoding::encode_truecolor_png(&image)?;
  let result = CapturedTextRegion {
    height: image.height,
    image_png,
    width: image.width,
  };
  Ok(result)
}

#[tauri::command]
pub async fn recognize_captured_text(
  state: tauri::State<'_, TextRecognitionState>,
) -> Result<TextRecognitionResult, String> {
  let image = state
    .selected()
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
