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

#[cfg(target_os = "windows")]
mod windows_platform {
  use std::{
    collections::{HashMap, HashSet},
    ffi::{c_void, OsStr, OsString},
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
  };

  use image::{ImageBuffer, Rgba};
  use rapidfuzz::fuzz::ratio;
  use windows::{
    core::PCWSTR,
    Win32::{
      Foundation::{CloseHandle, HWND, RECT},
      Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetMonitorInfoW, GetObjectW,
        MonitorFromWindow, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        MONITORINFO, MONITOR_DEFAULTTONEAREST,
      },
      System::{
        ProcessStatus::K32GetModuleFileNameExW,
        Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
      },
      UI::{
        Shell::ExtractIconExW,
        WindowsAndMessaging::{
          DestroyIcon, GetIconInfo, GetWindowLongPtrW, GetWindowRect, SetWindowLongPtrW,
          SetWindowPos, GWL_EXSTYLE, GWL_STYLE, ICONINFO, SWP_FRAMECHANGED, SWP_NOACTIVATE,
          SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_BORDER, WS_CAPTION, WS_DLGFRAME,
          WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME, WS_EX_STATICEDGE, WS_EX_WINDOWEDGE,
          WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_OVERLAPPEDWINDOW, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
        },
      },
    },
  };

  static ORIGINAL_STYLES: OnceLock<Mutex<HashMap<isize, (isize, isize)>>> = OnceLock::new();

  pub fn selectable_window_ids() -> Option<HashSet<u32>> {
    None
  }

  fn find_window(id: u32, pid: u32, title: &str) -> Result<HWND, String> {
    let windows = xcap::Window::all().map_err(|error| error.to_string())?;
    windows
      .into_iter()
      .filter(|window| window.pid().ok() == Some(pid))
      .max_by(|left, right| {
        let score = |window: &xcap::Window| {
          if window.id().ok() == Some(id) {
            f64::MAX
          } else {
            ratio(window.title().unwrap_or_default().chars(), title.chars())
          }
        };
        score(left).total_cmp(&score(right))
      })
      .and_then(|window| window.id().ok())
      .map(|window_id| HWND(window_id as usize as *mut c_void))
      .ok_or_else(|| format!("Could not find window '{title}'"))
  }

  pub fn app_icon(cache_dir: &Path, pid: u32) -> Option<PathBuf> {
    unsafe {
      let process = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid).ok()?;
      let mut buffer = [0_u16; 260];
      let length = K32GetModuleFileNameExW(Some(process), None, &mut buffer);
      let _ = CloseHandle(process);
      if length == 0 {
        return None;
      }

      let executable = PathBuf::from(OsString::from_wide(&buffer[..length as usize]));
      let name = executable.file_stem()?.to_string_lossy();
      let path = cache_dir.join(format!("app-{name}.png"));
      if path.exists() {
        return Some(path);
      }
      let executable_wide = OsStr::new(&executable)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
      let mut icon = Default::default();
      if ExtractIconExW(
        PCWSTR::from_raw(executable_wide.as_ptr()),
        0,
        Some(&mut icon),
        None,
        1,
      ) == 0
      {
        return None;
      }

      let mut info = ICONINFO::default();
      if GetIconInfo(icon, &mut info).is_err() {
        let _ = DestroyIcon(icon);
        return None;
      }
      let dc = CreateCompatibleDC(None);
      let mut bitmap = BITMAP::default();
      if GetObjectW(
        info.hbmColor.into(),
        std::mem::size_of::<BITMAP>() as i32,
        Some(&mut bitmap as *mut _ as *mut _),
      ) == 0
      {
        cleanup_icon(icon, info, dc);
        return None;
      }
      let mut bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
          biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
          biWidth: bitmap.bmWidth,
          biHeight: -bitmap.bmHeight,
          biPlanes: 1,
          biBitCount: 32,
          biCompression: BI_RGB.0,
          ..Default::default()
        },
        ..Default::default()
      };
      let mut pixels = vec![0_u8; (bitmap.bmWidth * bitmap.bmHeight * 4) as usize];
      let lines = GetDIBits(
        dc,
        info.hbmColor,
        0,
        bitmap.bmHeight as u32,
        Some(pixels.as_mut_ptr().cast()),
        &mut bitmap_info,
        DIB_RGB_COLORS,
      );
      if lines == 0 {
        cleanup_icon(icon, info, dc);
        return None;
      }
      for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
      }
      if pixels.chunks_exact(4).all(|pixel| pixel[3] == 0) {
        for pixel in pixels.chunks_exact_mut(4) {
          pixel[3] = 255;
        }
      }
      cleanup_icon(icon, info, dc);
      ImageBuffer::<Rgba<u8>, _>::from_raw(bitmap.bmWidth as u32, bitmap.bmHeight as u32, pixels)?
        .save(&path)
        .ok()?;
      Some(path)
    }
  }

  pub fn app_identity(pid: u32) -> Option<String> {
    unsafe {
      let process = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid).ok()?;
      let mut buffer = [0_u16; 260];
      let length = K32GetModuleFileNameExW(Some(process), None, &mut buffer);
      let _ = CloseHandle(process);
      (length > 0).then(|| {
        OsString::from_wide(&buffer[..length as usize])
          .to_string_lossy()
          .to_lowercase()
      })
    }
  }

  unsafe fn cleanup_icon(
    icon: windows::Win32::UI::WindowsAndMessaging::HICON,
    info: ICONINFO,
    dc: windows::Win32::Graphics::Gdi::HDC,
  ) {
    unsafe {
      let _ = DeleteObject(info.hbmColor.into());
      let _ = DeleteObject(info.hbmMask.into());
      let _ = DestroyIcon(icon);
      let _ = DeleteDC(dc);
    }
  }

  pub fn resize_window(
    id: u32,
    pid: u32,
    title: &str,
    width: u32,
    height: u32,
  ) -> Result<(), String> {
    let window = find_window(id, pid, title)?;
    unsafe {
      SetWindowPos(
        window,
        None,
        0,
        0,
        width as i32,
        height as i32,
        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
      )
      .map_err(|error| error.to_string())
    }
  }

  pub fn center_window(id: u32, pid: u32, title: &str) -> Result<(), String> {
    let window = find_window(id, pid, title)?;
    unsafe {
      let mut bounds = RECT::default();
      GetWindowRect(window, &mut bounds).map_err(|error| error.to_string())?;
      let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
      let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
      };
      if !GetMonitorInfoW(monitor, &mut info).as_bool() {
        return Err("Could not read the window's display work area".into());
      }
      let width = bounds.right - bounds.left;
      let height = bounds.bottom - bounds.top;
      SetWindowPos(
        window,
        None,
        info.rcWork.left + (info.rcWork.right - info.rcWork.left - width) / 2,
        info.rcWork.top + (info.rcWork.bottom - info.rcWork.top - height) / 2,
        0,
        0,
        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
      )
      .map_err(|error| error.to_string())
    }
  }

  pub fn make_borderless(id: u32, pid: u32, title: &str) -> Result<(), String> {
    let window = find_window(id, pid, title)?;
    unsafe {
      let style = GetWindowLongPtrW(window, GWL_STYLE);
      let extended_style = GetWindowLongPtrW(window, GWL_EXSTYLE);
      ORIGINAL_STYLES
        .get_or_init(Default::default)
        .lock()
        .map_err(|error| error.to_string())?
        .entry(window.0 as isize)
        .or_insert((style, extended_style));

      let style = (style
        & !(WS_OVERLAPPEDWINDOW.0
          | WS_CAPTION.0
          | WS_BORDER.0
          | WS_DLGFRAME.0
          | WS_THICKFRAME.0
          | WS_MINIMIZEBOX.0
          | WS_MAXIMIZEBOX.0
          | WS_SYSMENU.0) as isize)
        | WS_POPUP.0 as isize;
      let extended_style = extended_style
        & !(WS_EX_DLGMODALFRAME.0 | WS_EX_CLIENTEDGE.0 | WS_EX_STATICEDGE.0 | WS_EX_WINDOWEDGE.0)
          as isize;
      SetWindowLongPtrW(window, GWL_STYLE, style);
      SetWindowLongPtrW(window, GWL_EXSTYLE, extended_style);
      SetWindowPos(
        window,
        None,
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
      )
      .map_err(|error| error.to_string())
    }
  }

  pub fn restore_border(id: u32, pid: u32, title: &str) -> Result<(), String> {
    let window = find_window(id, pid, title)?;
    let original = ORIGINAL_STYLES
      .get_or_init(Default::default)
      .lock()
      .map_err(|error| error.to_string())?
      .remove(&(window.0 as isize));
    unsafe {
      let (style, extended_style) = original.unwrap_or_else(|| {
        (
          GetWindowLongPtrW(window, GWL_STYLE) | WS_OVERLAPPEDWINDOW.0 as isize,
          GetWindowLongPtrW(window, GWL_EXSTYLE)
            | WS_EX_WINDOWEDGE.0 as isize
            | WS_EX_CLIENTEDGE.0 as isize,
        )
      });
      SetWindowLongPtrW(window, GWL_STYLE, style);
      SetWindowLongPtrW(window, GWL_EXSTYLE, extended_style);
      SetWindowPos(
        window,
        None,
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
      )
      .map_err(|error| error.to_string())
    }
  }
}

#[cfg(target_os = "windows")]
pub use windows_platform::{
  app_icon, app_identity, center_window, make_borderless, resize_window, restore_border,
  selectable_window_ids,
};
