// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

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
    core::{PCWSTR, PWSTR},
    Win32::{
      Foundation::{CloseHandle, HWND, RECT},
      Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetMonitorInfoW, GetObjectW,
        MonitorFromWindow, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        MONITORINFO, MONITOR_DEFAULTTONEAREST,
      },
      System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
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

  /// Full path of a process's executable image.
  ///
  /// Uses `PROCESS_QUERY_LIMITED_INFORMATION` + `QueryFullProcessImageNameW`
  /// rather than `PROCESS_QUERY_INFORMATION | PROCESS_VM_READ` +
  /// `GetModuleFileNameEx`: the heavier rights are denied when the target runs
  /// at a higher integrity level (e.g. an elevated app while we are not), which
  /// left elevated windows with no icon at all. The limited right crosses that
  /// boundary, and reading the icon afterwards touches the file on disk, not the
  /// process, so it needs nothing more.
  fn process_image_path(pid: u32) -> Option<PathBuf> {
    unsafe {
      let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
      let mut buffer = [0_u16; 1024];
      let mut length = buffer.len() as u32;
      let result = QueryFullProcessImageNameW(
        process,
        PROCESS_NAME_WIN32,
        PWSTR::from_raw(buffer.as_mut_ptr()),
        &mut length,
      );
      let _ = CloseHandle(process);
      result.ok()?;
      (length > 0).then(|| PathBuf::from(OsString::from_wide(&buffer[..length as usize])))
    }
  }

  pub fn app_icon(cache_dir: &Path, pid: u32) -> Option<PathBuf> {
    let executable = process_image_path(pid)?;
    let name = executable.file_stem()?.to_string_lossy();
    let path = cache_dir.join(format!("app-{name}.png"));
    if path.exists() {
      return Some(path);
    }
    unsafe {
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
    Some(process_image_path(pid)?.to_string_lossy().to_lowercase())
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
