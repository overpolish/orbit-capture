// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Non-activating native input window for the DirectComposition workspace.

use std::sync::OnceLock;

use windows::{
  core::{w, PCWSTR},
  Win32::{
    Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM},
    Graphics::Gdi::ScreenToClient,
    System::LibraryLoader::GetModuleHandleW,
    UI::Input::KeyboardAndMouse::{GetKeyState, ReleaseCapture, SetCapture, VK_MENU},
    UI::WindowsAndMessaging::{
      CreateWindowExW, DefWindowProcW, DestroyWindow, LoadCursorW, RegisterClassW, SetCursor,
      SetWindowPos, ShowWindow, CS_DBLCLKS, CW_USEDEFAULT, HMENU, HTCLIENT, IDC_ARROW, IDC_SIZEALL,
      IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE, MA_NOACTIVATE, SWP_NOACTIVATE,
      SWP_NOOWNERZORDER, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE, WM_CANCELMODE,
      WM_CAPTURECHANGED, WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP,
      WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCHITTEST, WM_SETCURSOR, WNDCLASSW,
      WS_CHILD, WS_CLIPSIBLINGS, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP,
    },
  },
};

#[derive(Clone, Copy)]
pub(super) enum CursorKind {
  Arrow,
  Move,
  ResizeHorizontal,
  ResizeVertical,
  ResizeNesw,
  ResizeNwse,
}

#[derive(Clone, Copy)]
pub(super) enum Input {
  DoubleClick {
    x: f64,
    y: f64,
  },
  Down {
    centered: bool,
    x: f64,
    y: f64,
    snapping: bool,
  },
  Move {
    centered: bool,
    x: f64,
    y: f64,
    pressed: bool,
    snapping: bool,
  },
  Cancel,
  Up {
    x: f64,
    y: f64,
  },
  Wheel {
    x: f64,
    y: f64,
    delta: f64,
  },
}

pub(super) struct EditorWindow {
  hwnd: HWND,
}

unsafe impl Send for EditorWindow {}
unsafe impl Sync for EditorWindow {}

impl EditorWindow {
  pub(super) fn new(parent: HWND) -> Result<Self, String> {
    let instance = unsafe { GetModuleHandleW(None) }.map_err(|error| error.to_string())?;
    let atom = *CLASS.get_or_init(|| unsafe {
      RegisterClassW(&WNDCLASSW {
        style: CS_DBLCLKS,
        lpfnWndProc: Some(window_proc),
        hInstance: HINSTANCE(instance.0),
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
        lpszClassName: w!("ScreenwidePreviewEditor"),
        ..Default::default()
      })
    });
    if atom == 0 {
      return Err("The Windows preview editor class could not be registered".to_owned());
    }
    let hwnd = unsafe {
      CreateWindowExW(
        WS_EX_NOACTIVATE | WS_EX_NOREDIRECTIONBITMAP,
        w!("ScreenwidePreviewEditor"),
        PCWSTR::null(),
        WS_CHILD | WS_CLIPSIBLINGS,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        1,
        1,
        Some(parent),
        Some(HMENU::default()),
        Some(HINSTANCE(instance.0)),
        None,
      )
    }
    .map_err(|error| format!("The Windows preview editor could not be created: {error}"))?;
    Ok(Self { hwnd })
  }

  pub(super) fn set_active(&self, active: bool) {
    let _ = unsafe { ShowWindow(self.hwnd, if active { SW_SHOWNOACTIVATE } else { SW_HIDE }) };
  }

  pub(super) fn set_frame(&self, x: i32, y: i32, width: i32, height: i32, active: bool) {
    let flags =
      SWP_NOACTIVATE | SWP_NOOWNERZORDER | active.then_some(SWP_SHOWWINDOW).unwrap_or_default();
    let _ = unsafe { SetWindowPos(self.hwnd, None, x, y, width.max(1), height.max(1), flags) };
  }

  pub(super) fn set_cursor(kind: CursorKind) {
    let name = match kind {
      CursorKind::Arrow => IDC_ARROW,
      CursorKind::Move => IDC_SIZEALL,
      CursorKind::ResizeHorizontal => IDC_SIZEWE,
      CursorKind::ResizeVertical => IDC_SIZENS,
      CursorKind::ResizeNesw => IDC_SIZENESW,
      CursorKind::ResizeNwse => IDC_SIZENWSE,
    };
    if let Ok(cursor) = unsafe { LoadCursorW(None, name) } {
      unsafe { SetCursor(Some(cursor)) };
    }
  }
}

impl Drop for EditorWindow {
  fn drop(&mut self) {
    let _ = unsafe { DestroyWindow(self.hwnd) };
  }
}

static CLASS: OnceLock<u16> = OnceLock::new();
const MK_CONTROL_MASK: usize = 0x0008;

fn option_pressed() -> bool {
  (unsafe { GetKeyState(VK_MENU.0 as i32) }) < 0
}

fn point(lparam: LPARAM) -> (f64, f64) {
  let x = (lparam.0 as u16 as i16) as f64;
  let y = ((lparam.0 >> 16) as u16 as i16) as f64;
  (x, y)
}

unsafe extern "system" fn window_proc(
  hwnd: HWND,
  message: u32,
  wparam: WPARAM,
  lparam: LPARAM,
) -> LRESULT {
  match message {
    WM_DESTROY => LRESULT(0),
    WM_NCHITTEST => LRESULT(HTCLIENT as isize),
    WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
    WM_LBUTTONDOWN => {
      SetCapture(hwnd);
      let (x, y) = point(lparam);
      super::handle_editor_input(Input::Down {
        centered: option_pressed(),
        x,
        y,
        snapping: wparam.0 & MK_CONTROL_MASK != 0,
      });
      LRESULT(0)
    }
    WM_LBUTTONDBLCLK => {
      let (x, y) = point(lparam);
      super::handle_editor_input(Input::DoubleClick { x, y });
      LRESULT(0)
    }
    WM_MOUSEMOVE => {
      let (x, y) = point(lparam);
      super::handle_editor_input(Input::Move {
        centered: option_pressed(),
        x,
        y,
        pressed: wparam.0 & 1 != 0,
        snapping: wparam.0 & MK_CONTROL_MASK != 0,
      });
      LRESULT(0)
    }
    WM_LBUTTONUP => {
      let (x, y) = point(lparam);
      super::handle_editor_input(Input::Up { x, y });
      let _ = ReleaseCapture();
      LRESULT(0)
    }
    WM_CANCELMODE | WM_CAPTURECHANGED => {
      super::handle_editor_input(Input::Cancel);
      LRESULT(0)
    }
    WM_MOUSEWHEEL => {
      let (screen_x, screen_y) = point(lparam);
      let mut local = POINT {
        x: screen_x as i32,
        y: screen_y as i32,
      };
      let _ = ScreenToClient(hwnd, &mut local);
      let delta = ((wparam.0 >> 16) as u16 as i16) as f64 / 120.0;
      super::handle_editor_input(Input::Wheel {
        x: f64::from(local.x),
        y: f64::from(local.y),
        delta,
      });
      LRESULT(0)
    }
    WM_SETCURSOR => {
      super::refresh_editor_cursor();
      LRESULT(1)
    }
    _ => DefWindowProcW(hwnd, message, wparam, lparam),
  }
}
