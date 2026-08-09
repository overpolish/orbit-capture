// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#[cfg(target_os = "macos")]
use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
use {
  cidre::{ax, cf, cg, sc},
  objc2::AnyThread,
  objc2_app_kit::{
    NSApplicationActivationPolicy, NSBitmapImageFileType, NSBitmapImageRep, NSRunningApplication,
  },
  objc2_foundation::{NSDictionary, NSString},
  rapidfuzz::fuzz::ratio,
  std::collections::HashSet,
};

#[cfg(target_os = "macos")]
pub struct AudioApplication {
  pub id: String,
  pub label: String,
  pub pid: u32,
}

#[cfg(target_os = "macos")]
pub async fn audio_applications() -> Result<Vec<AudioApplication>, String> {
  let current_pid = std::process::id();
  let content = sc::ShareableContent::current()
    .await
    .map_err(|error| error.to_string())?;

  Ok(
    content
      .apps()
      .iter()
      .filter_map(|application| {
        let pid = u32::try_from(application.process_id()).ok()?;
        let running_application =
          NSRunningApplication::runningApplicationWithProcessIdentifier(pid as i32)?;
        if running_application.activationPolicy() != NSApplicationActivationPolicy::Regular {
          return None;
        }
        let id = application.bundle_id().to_string();
        let label = application.app_name().to_string();
        if pid == current_pid || id.trim().is_empty() || label.trim().is_empty() {
          return None;
        }
        Some(AudioApplication {
          id,
          label: label.trim().to_owned(),
          pid,
        })
      })
      .collect(),
  )
}

#[cfg(target_os = "macos")]
pub fn app_icon(cache_dir: &Path, pid: u32) -> Option<PathBuf> {
  unsafe {
    let running_app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid as i32)?;
    let bundle_id = running_app.bundleIdentifier()?.to_string();
    let path = cache_dir.join(format!("app-{}.png", sanitize_filename(&bundle_id)));
    if path.exists() {
      return Some(path);
    }

    let icon = running_app.icon()?;
    let cg_image = icon.CGImageForProposedRect_context_hints(std::ptr::null_mut(), None, None)?;
    let bitmap = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &cg_image);
    let properties = NSDictionary::new();
    let png = bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties)?;
    let path_string = NSString::from_str(&path.to_string_lossy());
    png
      .writeToFile_atomically(&path_string, true)
      .then_some(path)
  }
}

#[cfg(target_os = "macos")]
pub fn selectable_window_ids() -> Option<HashSet<u32>> {
  let options = cg::WindowListOpt::ON_SCREEN_ONLY | cg::WindowListOpt::EXCLUDE_DESKTOP_ELEMENTS;
  let windows = cg::WindowList::info(options, cg::WINDOW_ID_NULL)?;
  Some(
    windows
      .iter()
      .filter_map(|window| {
        let layer = window
          .get(cg::window_keys::layer())?
          .try_as_number()?
          .to_i32()?;
        if layer != 0 {
          return None;
        }
        window
          .get(cg::window_keys::number())?
          .try_as_number()?
          .to_i32()
          .map(|id| id as u32)
      })
      .collect(),
  )
}

#[cfg(target_os = "macos")]
fn sanitize_filename(value: &str) -> String {
  value
    .chars()
    .map(|character| {
      if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
        character
      } else {
        '_'
      }
    })
    .collect()
}

#[cfg(target_os = "macos")]
fn find_ax_window(pid: u32, title: &str) -> Result<(cidre::arc::R<ax::UiElement>, usize), String> {
  let app = ax::UiElement::with_app_pid(pid as i32);
  let windows = app.children().map_err(|error| error.to_string())?;
  let mut best = None;

  for (index, window) in windows.iter().enumerate() {
    if window
      .role()
      .ok()
      .is_none_or(|role| role.to_string() != "AXWindow")
    {
      continue;
    }
    let Ok(value) = window.attr_value(ax::attr::title()) else {
      continue;
    };
    let current_title: cidre::arc::R<cf::String> = unsafe { cf::Type::retain(&value) };
    let score = ratio(current_title.to_string().chars(), title.chars());
    if best.is_none_or(|(_, best_score)| score > best_score) {
      best = Some((index, score));
    }
  }

  best
    .map(|(index, _)| (app, index))
    .ok_or_else(|| format!("Could not find an accessible window for process {pid}"))
}

#[cfg(target_os = "macos")]
pub fn resize_window(
  _id: u32,
  pid: u32,
  title: &str,
  width: u32,
  height: u32,
) -> Result<(), String> {
  let (app, index) = find_ax_window(pid, title)?;
  let mut windows = app.children().map_err(|error| error.to_string())?;
  let window = &mut windows[index];
  if !window
    .is_settable(ax::attr::size())
    .map_err(|error| error.to_string())?
  {
    return Err("The selected application does not allow resizing this window".into());
  }

  let size = ax::Value::with_cg_size(&cg::Size {
    width: f64::from(width),
    height: f64::from(height),
  });
  window
    .set_attr(ax::attr::size(), size.as_ref())
    .map_err(|error| error.to_string())
}

#[cfg(target_os = "macos")]
pub fn center_window(_id: u32, pid: u32, title: &str) -> Result<(), String> {
  let (app, index) = find_ax_window(pid, title)?;
  let mut windows = app.children().map_err(|error| error.to_string())?;
  let window = &mut windows[index];
  let position = window.pos().map_err(|error| error.to_string())?;
  let size = window.size().map_err(|error| error.to_string())?;
  let position = position
    .cg_point()
    .ok_or_else(|| "Could not read the selected window position".to_string())?;
  let size = size
    .cg_size()
    .ok_or_else(|| "Could not read the selected window size".to_string())?;

  let monitor = xcap::Monitor::all()
    .map_err(|error| error.to_string())?
    .into_iter()
    .max_by(|left, right| {
      intersection_area(left, position, size).total_cmp(&intersection_area(right, position, size))
    })
    .ok_or_else(|| "No display is available".to_string())?;
  let monitor_x = f64::from(monitor.x().map_err(|error| error.to_string())?);
  let monitor_y = f64::from(monitor.y().map_err(|error| error.to_string())?);
  let monitor_width = f64::from(monitor.width().map_err(|error| error.to_string())?);
  let monitor_height = f64::from(monitor.height().map_err(|error| error.to_string())?);
  let target = ax::Value::with_cg_point(&cg::Point {
    x: monitor_x + (monitor_width - size.width) / 2.0,
    y: monitor_y + (monitor_height - size.height) / 2.0,
  });
  window
    .set_attr(ax::attr::pos(), target.as_ref())
    .map_err(|error| error.to_string())
}

#[cfg(target_os = "macos")]
fn intersection_area(monitor: &xcap::Monitor, position: cg::Point, size: cg::Size) -> f64 {
  let Ok(x) = monitor.x() else { return 0.0 };
  let Ok(y) = monitor.y() else { return 0.0 };
  let Ok(width) = monitor.width() else {
    return 0.0;
  };
  let Ok(height) = monitor.height() else {
    return 0.0;
  };
  let left = position.x.max(f64::from(x));
  let top = position.y.max(f64::from(y));
  let right = (position.x + size.width).min(f64::from(x) + f64::from(width));
  let bottom = (position.y + size.height).min(f64::from(y) + f64::from(height));
  (right - left).max(0.0) * (bottom - top).max(0.0)
}
