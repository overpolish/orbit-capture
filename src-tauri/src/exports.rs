mod window;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};
use serde::Serialize;
use tauri::{image::Image, ipc::Response, AppHandle, Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::screenshots::{encode_png, screenshot_directory, unique_path, CapturedImage};

const EXPORT_CHANGED_EVENT: &str = "export://artifact";
const EXPORT_DIRECTORY_FILE: &str = "export-directory.json";
const FILE_EXTENSION: &str = "png";
/// The long edge of the preview shipped to the window. The capture itself can
/// be 30 MB of pixels, which has no business crossing the IPC boundary.
const PREVIEW_MAX_EDGE: u32 = 640;
const MAX_FILE_STEM: usize = 200;

/// A capture waiting to be saved.
///
/// Recordings become a second variant here, which is why the window renders
/// itself by artifact kind rather than assuming a screenshot.
pub enum ExportArtifact {
  Screenshot {
    /// Unique per capture. Two consecutive fullscreen captures are identical
    /// in every other respect, so the window needs this to tell them apart
    /// and start the new one at fit rather than inheriting the old zoom.
    id: u64,
    image: CapturedImage,
    suggested_file_stem: String,
  },
}

/// What the window is told about the pending artifact. Deliberately without
/// pixels: the preview travels separately, as bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(
  rename_all = "camelCase",
  rename_all_fields = "camelCase",
  tag = "kind"
)]
pub enum ExportArtifactSnapshot {
  Screenshot {
    id: u64,
    suggested_file_stem: String,
    extension: String,
    width: u32,
    height: u32,
  },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSnapshot {
  pub artifact: Option<ExportArtifactSnapshot>,
  pub directory: Option<PathBuf>,
}

#[derive(Default)]
pub struct ExportState {
  artifact: Mutex<Option<ExportArtifact>>,
  generation: AtomicU64,
  preview: Mutex<Option<Vec<u8>>>,
  /// Built only if the user zooms in, because it is the whole capture.
  full_preview: Mutex<Option<Vec<u8>>>,
  directory: Mutex<Option<PathBuf>>,
}

/// Characters Windows forbids outright. macOS only objects to `/` and `:`, so
/// stripping the Windows set keeps a name portable between the two.
const ILLEGAL_CHARACTERS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Names Windows reserves whatever the extension is.
const RESERVED_STEMS: &[&str] = &[
  "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
  "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Cleans a user-typed file name into something both platforms will accept, or
/// `None` if nothing usable is left.
///
/// Illegal characters are stripped rather than rejected: a name is a label, and
/// silently dropping a colon is friendlier than refusing to save over one.
pub fn sanitize_file_stem(input: &str) -> Option<String> {
  let stripped: String = input
    .chars()
    .filter(|character| !ILLEGAL_CHARACTERS.contains(character) && !character.is_control())
    .collect();
  // Windows silently drops trailing dots and spaces, which would leave the
  // saved file under a different name than the one shown.
  let trimmed = stripped.trim().trim_end_matches(['.', ' ']).trim();

  if trimmed.is_empty() {
    return None;
  }
  if RESERVED_STEMS
    .iter()
    .any(|reserved| trimmed.eq_ignore_ascii_case(reserved))
  {
    return None;
  }

  let mut stem = trimmed.to_owned();
  if stem.len() > MAX_FILE_STEM {
    stem = stem.chars().take(MAX_FILE_STEM).collect::<String>();
    stem = stem.trim().to_owned();
  }

  (!stem.is_empty()).then_some(stem)
}

fn directory_path(app: &AppHandle) -> tauri::Result<PathBuf> {
  Ok(app.path().app_config_dir()?.join(EXPORT_DIRECTORY_FILE))
}

fn load_directory(app: &AppHandle) -> Option<PathBuf> {
  let stored = directory_path(app)
    .ok()
    .and_then(|path| std::fs::read(path).ok())
    .and_then(|contents| serde_json::from_slice::<PathBuf>(&contents).ok())?;

  stored.is_dir().then_some(stored)
}

fn store_directory(app: &AppHandle, directory: &Path) -> tauri::Result<()> {
  let path = directory_path(app)?;
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)?;
  }
  let contents = serde_json::to_vec_pretty(directory).map_err(std::io::Error::other)?;
  std::fs::write(path, contents)?;

  Ok(())
}

/// The folder the next export lands in: whatever was used last, falling back to
/// the platform's own screenshot folder on a first run.
fn current_directory(app: &AppHandle) -> Option<PathBuf> {
  let state = app.state::<ExportState>();
  let remembered = state
    .directory
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .clone();

  remembered.or_else(|| screenshot_directory(app).ok())
}

fn snapshot(app: &AppHandle) -> ExportSnapshot {
  let state = app.state::<ExportState>();
  let artifact = state
    .artifact
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .as_ref()
    .map(|artifact| match artifact {
      ExportArtifact::Screenshot {
        id,
        image,
        suggested_file_stem,
      } => ExportArtifactSnapshot::Screenshot {
        id: *id,
        suggested_file_stem: suggested_file_stem.clone(),
        extension: FILE_EXTENSION.to_owned(),
        width: image.width,
        height: image.height,
      },
    });

  ExportSnapshot {
    artifact,
    directory: current_directory(app),
  }
}

fn emit_snapshot(app: &AppHandle) {
  let _ = app.emit(EXPORT_CHANGED_EVENT, snapshot(app));
}

/// Shrinks the capture to something worth sending over IPC.
fn preview_png(image: &CapturedImage) -> Option<Vec<u8>> {
  let buffer = image::RgbaImage::from_raw(image.width, image.height, image.rgba.clone())?;
  let scale = f64::from(PREVIEW_MAX_EDGE) / f64::from(image.width.max(image.height));
  let (width, height) = if scale >= 1.0 {
    (image.width, image.height)
  } else {
    (
      ((f64::from(image.width) * scale).round() as u32).max(1),
      ((f64::from(image.height) * scale).round() as u32).max(1),
    )
  };

  let thumbnail = image::DynamicImage::ImageRgba8(buffer).thumbnail(width, height);
  let mut png = Vec::new();
  thumbnail
    .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
    .ok()?;

  Some(png)
}

/// The capture at full resolution, for zooming into. Encoded losslessly and
/// quickly - this is for looking at, not for keeping, so the slow quantizing
/// encoder that produces the saved file would be the wrong trade here.
fn full_preview_png(image: &CapturedImage) -> Result<Vec<u8>, String> {
  let mut png = Vec::new();
  PngEncoder::new_with_quality(
    std::io::Cursor::new(&mut png),
    CompressionType::Fast,
    FilterType::Sub,
  )
  .write_image(
    &image.rgba,
    image.width,
    image.height,
    ExtendedColorType::Rgba8,
  )
  .map_err(|error| error.to_string())?;

  Ok(png)
}

/// Hands a freshly captured still to the export window, replacing whatever was
/// waiting there before.
pub fn present_screenshot(
  app: &AppHandle,
  image: CapturedImage,
  suggested_file_stem: String,
) -> Result<(), String> {
  let preview = preview_png(&image);
  {
    let state = app.state::<ExportState>();
    let id = state
      .generation
      .fetch_add(1, Ordering::SeqCst)
      .wrapping_add(1);
    *state
      .preview
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner()) = preview;
    *state
      .full_preview
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    *state
      .artifact
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(ExportArtifact::Screenshot {
      id,
      image,
      suggested_file_stem,
    });
  }

  window::show(app).map_err(|error| error.to_string())?;
  emit_snapshot(app);

  Ok(())
}

fn take_artifact(app: &AppHandle) -> Option<ExportArtifact> {
  let state = app.state::<ExportState>();
  let _ = state
    .preview
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .take();
  let _ = state
    .full_preview
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .take();

  let artifact = state
    .artifact
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .take();

  artifact
}

/// Drops the pending artifact and puts the window away. Cancelling and closing
/// the window are the same act.
pub fn discard(app: &AppHandle) {
  let _ = take_artifact(app);
  let _ = window::hide(app);
  emit_snapshot(app);
}

#[tauri::command]
pub fn get_export_snapshot(app: AppHandle) -> ExportSnapshot {
  snapshot(&app)
}

/// The thumbnail by default; the full-resolution capture only once something
/// actually needs it, and cached from then on.
#[tauri::command]
pub async fn get_export_preview(app: AppHandle, full: bool) -> Result<Response, String> {
  let bytes = tauri::async_runtime::spawn_blocking(move || {
    let state = app.state::<ExportState>();
    let missing = || "There is nothing waiting to be exported".to_owned();

    if !full {
      return state
        .preview
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .ok_or_else(missing);
    }

    if let Some(cached) = state
      .full_preview
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .clone()
    {
      return Ok(cached);
    }

    let encoded = {
      let artifact = state
        .artifact
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
      let Some(ExportArtifact::Screenshot { image, .. }) = artifact.as_ref() else {
        return Err(missing());
      };
      full_preview_png(image)?
    };
    *state
      .full_preview
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(encoded.clone());

    Ok(encoded)
  })
  .await
  .map_err(|error| error.to_string())??;

  Ok(Response::new(bytes))
}

#[tauri::command]
pub fn cancel_export(app: AppHandle) {
  discard(&app);
}

#[tauri::command]
pub fn copy_export_to_clipboard(app: AppHandle) -> Result<(), String> {
  let artifact = take_artifact(&app).ok_or_else(|| "There is nothing to copy".to_owned())?;
  let ExportArtifact::Screenshot { image, .. } = artifact;

  app
    .clipboard()
    .write_image(&Image::new(&image.rgba, image.width, image.height))
    .map_err(|error| error.to_string())?;

  let _ = window::hide(&app);
  emit_snapshot(&app);

  Ok(())
}

#[tauri::command]
pub async fn browse_export_directory(app: AppHandle) -> Result<Option<PathBuf>, String> {
  let start = current_directory(&app);
  // Parented to the export window on purpose: left to itself the picker
  // attaches as a sheet to whichever window happens to be first, which for an
  // accessory app is usually one of the hidden overlay panels - and a sheet on
  // a hidden window is an invisible dialog.
  let parent = app.get_webview_window(crate::windows::WindowLabel::Export.as_str());
  let picked = tauri::async_runtime::spawn_blocking(move || {
    use tauri_plugin_dialog::DialogExt;

    let mut dialog = app.dialog().file().set_title("Choose a folder");
    if let Some(start) = start {
      dialog = dialog.set_directory(start);
    }
    if let Some(parent) = &parent {
      dialog = dialog.set_parent(parent);
    }
    dialog.blocking_pick_folder()
  })
  .await
  .map_err(|error| error.to_string())?;

  Ok(picked.and_then(|path| path.into_path().ok()))
}

#[tauri::command]
pub fn set_export_directory(app: AppHandle, directory: PathBuf) -> Result<(), String> {
  if !directory.is_dir() {
    return Err("That folder is no longer available".to_owned());
  }

  *app
    .state::<ExportState>()
    .directory
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(directory.clone());
  store_directory(&app, &directory).map_err(|error| error.to_string())?;
  emit_snapshot(&app);

  Ok(())
}

#[tauri::command]
pub async fn save_export(app: AppHandle, file_stem: String) -> Result<PathBuf, String> {
  let stem =
    sanitize_file_stem(&file_stem).ok_or_else(|| "That file name cannot be used".to_owned())?;
  let directory =
    current_directory(&app).ok_or_else(|| "There is nowhere to save this".to_owned())?;
  let artifact = take_artifact(&app).ok_or_else(|| "There is nothing to save".to_owned())?;
  let ExportArtifact::Screenshot { image, .. } = artifact;

  let writing = directory.clone();
  let path = tauri::async_runtime::spawn_blocking(move || {
    std::fs::create_dir_all(&writing).map_err(|error| error.to_string())?;
    let path = unique_path(&writing, &stem, FILE_EXTENSION, &|candidate| {
      candidate.exists()
    });
    std::fs::write(&path, encode_png(&image)?).map_err(|error| error.to_string())?;
    Ok::<PathBuf, String>(path)
  })
  .await
  .map_err(|error| error.to_string())??;

  // Only remembered once a save actually lands there.
  set_export_directory(app.clone(), directory)?;
  let _ = window::hide(&app);
  emit_snapshot(&app);

  Ok(path)
}

pub fn initialize(app: &AppHandle) {
  if let Some(directory) = load_directory(app) {
    *app
      .state::<ExportState>()
      .directory
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(directory);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn keeps_a_reasonable_name_untouched() {
    assert_eq!(
      sanitize_file_stem("Orbit Capture 2026-08-08 at 14.32.05").as_deref(),
      Some("Orbit Capture 2026-08-08 at 14.32.05")
    );
  }

  #[test]
  fn strips_characters_neither_platform_allows() {
    assert_eq!(
      sanitize_file_stem(r#"a<b>c:d"e/f\g|h?i*j"#).as_deref(),
      Some("abcdefghij")
    );
  }

  #[test]
  fn strips_control_characters() {
    assert_eq!(
      sanitize_file_stem("one\ttwo\nthree").as_deref(),
      Some("onetwothree")
    );
  }

  #[test]
  fn trims_surrounding_whitespace() {
    assert_eq!(sanitize_file_stem("   shot   ").as_deref(), Some("shot"));
  }

  #[test]
  fn drops_trailing_dots_and_spaces_that_windows_would_eat() {
    assert_eq!(sanitize_file_stem("shot. . .").as_deref(), Some("shot"));
    assert_eq!(sanitize_file_stem("shot   ").as_deref(), Some("shot"));
  }

  #[test]
  fn rejects_a_name_with_nothing_left_in_it() {
    assert_eq!(sanitize_file_stem(""), None);
    assert_eq!(sanitize_file_stem("   "), None);
    assert_eq!(sanitize_file_stem("///"), None);
    assert_eq!(sanitize_file_stem("..."), None);
  }

  #[test]
  fn rejects_names_windows_reserves() {
    assert_eq!(sanitize_file_stem("CON"), None);
    assert_eq!(sanitize_file_stem("nul"), None);
    assert_eq!(sanitize_file_stem("Com1"), None);
    assert_eq!(sanitize_file_stem("LPT9"), None);
    // Only the exact stem is reserved.
    assert_eq!(sanitize_file_stem("console").as_deref(), Some("console"));
  }

  #[test]
  fn caps_an_absurdly_long_name() {
    let stem = sanitize_file_stem(&"a".repeat(500)).unwrap();
    assert_eq!(stem.len(), MAX_FILE_STEM);
  }

  #[test]
  fn keeps_a_dot_inside_the_name() {
    assert_eq!(
      sanitize_file_stem("v1.2.3 build").as_deref(),
      Some("v1.2.3 build")
    );
  }
}
