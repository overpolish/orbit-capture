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
      CreateWindowExW, DefWindowProcW, DestroyWindow, GetAncestor, GetForegroundWindow,
      LoadCursorW, RegisterClassW, SetCursor, SetForegroundWindow, SetWindowPos, ShowWindowAsync,
      CS_DBLCLKS, CW_USEDEFAULT, GA_ROOT, HMENU, HTCLIENT, HWND_TOP, IDC_ARROW, IDC_SIZEALL,
      IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE, MA_NOACTIVATE, SWP_ASYNCWINDOWPOS,
      SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE,
      SW_SHOWNOACTIVATE, WM_CANCELMODE, WM_CAPTURECHANGED, WM_DESTROY, WM_LBUTTONDBLCLK,
      WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE,
      WM_MOUSEWHEEL, WM_NCHITTEST, WM_SETCURSOR, WNDCLASSW, WS_CHILD, WS_CLIPSIBLINGS,
      WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP,
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
// Payloads mirror the Win32 messages; not every field is consumed yet.
#[allow(dead_code)]
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
  /// Middle button: pans from wherever it lands, like any non-primary button
  /// on macOS. Trackpads are rare on Windows and the primary button is taken
  /// by selection over a pane.
  PanDown {
    x: f64,
    y: f64,
  },
  PanUp {
    x: f64,
    y: f64,
  },
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
    // A freshly created child lands at the bottom of the sibling z-order, so
    // raise it above WebView2 before the first `set_frame` arrives.
    raise(hwnd);
    Ok(Self { hwnd })
  }

  pub(super) fn hwnd(&self) -> HWND {
    self.hwnd
  }

  /// `ShowWindowAsync` posts rather than sends, so callers off the event-loop
  /// thread never block inside the main thread while holding surface state.
  pub(super) fn set_active(&self, active: bool) {
    let _ = unsafe { ShowWindowAsync(self.hwnd, if active { SW_SHOWNOACTIVATE } else { SW_HIDE }) };
    if active {
      raise(self.hwnd);
    }
  }

  /// `SWP_ASYNCWINDOWPOS` likewise posts the request when it arrives from a
  /// non-owning thread. The z-order is deliberately re-asserted to `HWND_TOP`
  /// on every move: the editor must stay above the sibling WebView2 child.
  pub(super) fn set_frame(&self, x: i32, y: i32, width: i32, height: i32, active: bool) {
    let flags = SWP_ASYNCWINDOWPOS
      | SWP_NOACTIVATE
      | SWP_NOOWNERZORDER
      | active.then_some(SWP_SHOWWINDOW).unwrap_or_default();
    let _ = unsafe {
      SetWindowPos(
        self.hwnd,
        Some(HWND_TOP),
        x,
        y,
        width.max(1),
        height.max(1),
        flags,
      )
    };
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
  // `DestroyWindow` only works on the creating thread; the surface lives in the
  // process-lifetime registry keyed by its host window and is never dropped in
  // practice.
  fn drop(&mut self) {
    let _ = unsafe { DestroyWindow(self.hwnd) };
  }
}

/// Re-asserts the editor above its WebView2 sibling without moving, sizing,
/// or activating it.
fn raise(hwnd: HWND) {
  let _ = unsafe {
    SetWindowPos(
      hwnd,
      Some(HWND_TOP),
      0,
      0,
      0,
      0,
      SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
    )
  };
}

static CLASS: OnceLock<u16> = OnceLock::new();
const MK_LBUTTON_MASK: usize = 0x0001;
const MK_CONTROL_MASK: usize = 0x0008;
const MK_MBUTTON_MASK: usize = 0x0010;

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
    WM_MOUSEACTIVATE => {
      // The editor itself never takes activation or focus (keyboard input
      // stays with the webview), but a click on the workspace still has to
      // raise the export window like a click anywhere else in it would.
      let root = GetAncestor(hwnd, GA_ROOT);
      if !root.is_invalid() && GetForegroundWindow() != root {
        let _ = SetForegroundWindow(root);
      }
      LRESULT(MA_NOACTIVATE as isize)
    }
    WM_LBUTTONDOWN => {
      SetCapture(hwnd);
      let (x, y) = point(lparam);
      dispatch(
        hwnd,
        Input::Down {
          centered: option_pressed(),
          x,
          y,
          snapping: wparam.0 & MK_CONTROL_MASK != 0,
        },
      );
      LRESULT(0)
    }
    WM_LBUTTONDBLCLK => {
      let (x, y) = point(lparam);
      dispatch(hwnd, Input::DoubleClick { x, y });
      LRESULT(0)
    }
    WM_MOUSEMOVE => {
      let (x, y) = point(lparam);
      dispatch(
        hwnd,
        Input::Move {
          centered: option_pressed(),
          x,
          y,
          pressed: wparam.0 & (MK_LBUTTON_MASK | MK_MBUTTON_MASK) != 0,
          snapping: wparam.0 & MK_CONTROL_MASK != 0,
        },
      );
      LRESULT(0)
    }
    WM_LBUTTONUP => {
      let (x, y) = point(lparam);
      dispatch(hwnd, Input::Up { x, y });
      let _ = ReleaseCapture();
      LRESULT(0)
    }
    WM_MBUTTONDOWN => {
      SetCapture(hwnd);
      let (x, y) = point(lparam);
      dispatch(hwnd, Input::PanDown { x, y });
      LRESULT(0)
    }
    WM_MBUTTONUP => {
      let (x, y) = point(lparam);
      dispatch(hwnd, Input::PanUp { x, y });
      let _ = ReleaseCapture();
      LRESULT(0)
    }
    WM_CANCELMODE | WM_CAPTURECHANGED => {
      dispatch(hwnd, Input::Cancel);
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
      dispatch(
        hwnd,
        Input::Wheel {
          x: f64::from(local.x),
          y: f64::from(local.y),
          delta,
        },
      );
      LRESULT(0)
    }
    WM_SETCURSOR => {
      guard(|| super::refresh_editor_cursor(hwnd));
      LRESULT(1)
    }
    _ => DefWindowProcW(hwnd, message, wparam, lparam),
  }
}

/// `window_proc` is an `extern "system"` callback: a panic that reaches it
/// cannot unwind and aborts the whole process. Contain gesture bugs to a
/// logged, dropped input instead.
fn guard(work: impl FnOnce()) {
  if std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).is_err() {
    eprintln!("The Windows preview editor dropped an input after a panic");
  }
}

fn dispatch(hwnd: HWND, input: Input) {
  guard(|| super::handle_editor_input(hwnd, input));
}
