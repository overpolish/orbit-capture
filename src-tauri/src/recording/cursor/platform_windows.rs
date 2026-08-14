// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Windows cursor event adapter. Cursor pixels are never captured here: the
//! sidecar stores semantic artwork plus geometry, matching the macOS format.

use std::{
  sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
  },
  thread::JoinHandle,
  time::{Duration, Instant},
};

use windows::Win32::{
  Graphics::Gdi::DeleteObject,
  UI::{
    Input::KeyboardAndMouse::{
      GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON, VK_XBUTTON1, VK_XBUTTON2,
    },
    WindowsAndMessaging::{
      GetCursorInfo, GetIconInfo, GetSystemMetrics, LoadCursorW, CURSORINFO, CURSOR_SHOWING,
      HCURSOR, IDC_ARROW, IDC_CROSS, IDC_HAND, IDC_IBEAM, IDC_NO, IDC_SIZENS, IDC_SIZEWE,
      SM_CXCURSOR, SM_CYCURSOR,
    },
  },
};

use super::{
  ButtonState, CursorAppearance, CursorButton, CursorStyle, EventSink, RawCursorEvent,
  RawCursorEventKind,
};

const POLL_INTERVAL: Duration = Duration::from_micros(7_500);
const START_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy)]
struct StandardCursors {
  arrow: HCURSOR,
  crosshair: HCURSOR,
  hand: HCURSOR,
  ibeam: HCURSOR,
  not_allowed: HCURSOR,
  resize_horizontal: HCURSOR,
  resize_vertical: HCURSOR,
}

impl StandardCursors {
  fn load() -> Result<Self, String> {
    let load = |name| unsafe { LoadCursorW(None, name) }.map_err(|error| error.to_string());
    Ok(Self {
      arrow: load(IDC_ARROW)?,
      crosshair: load(IDC_CROSS)?,
      hand: load(IDC_HAND)?,
      ibeam: load(IDC_IBEAM)?,
      not_allowed: load(IDC_NO)?,
      resize_horizontal: load(IDC_SIZEWE)?,
      resize_vertical: load(IDC_SIZENS)?,
    })
  }

  fn style(self, cursor: HCURSOR) -> CursorStyle {
    if cursor == self.arrow {
      CursorStyle::Arrow
    } else if cursor == self.crosshair {
      CursorStyle::Crosshair
    } else if cursor == self.hand {
      CursorStyle::PointingHand
    } else if cursor == self.ibeam {
      CursorStyle::IBeam
    } else if cursor == self.not_allowed {
      CursorStyle::NotAllowed
    } else if cursor == self.resize_horizontal {
      CursorStyle::ResizeHorizontal
    } else if cursor == self.resize_vertical {
      CursorStyle::ResizeVertical
    } else {
      CursorStyle::Custom
    }
  }
}

fn appearance(cursors: StandardCursors, cursor: HCURSOR) -> CursorAppearance {
  let mut hotspot = (0.0, 0.0);
  let mut info = windows::Win32::UI::WindowsAndMessaging::ICONINFO::default();
  if unsafe { GetIconInfo(cursor.into(), &mut info) }.is_ok() {
    hotspot = (f64::from(info.xHotspot), f64::from(info.yHotspot));
    if !info.hbmColor.is_invalid() {
      let _ = unsafe { DeleteObject(info.hbmColor.into()) };
    }
    if !info.hbmMask.is_invalid() {
      let _ = unsafe { DeleteObject(info.hbmMask.into()) };
    }
  }
  CursorAppearance {
    height: f64::from(unsafe { GetSystemMetrics(SM_CYCURSOR) }.max(1)),
    hotspot_x: hotspot.0,
    hotspot_y: hotspot.1,
    style: cursors.style(cursor),
    width: f64::from(unsafe { GetSystemMetrics(SM_CXCURSOR) }.max(1)),
  }
}

fn current_cursor(cursors: StandardCursors) -> Result<(f64, f64, HCURSOR), String> {
  let mut info = CURSORINFO {
    cbSize: size_of::<CURSORINFO>() as u32,
    ..Default::default()
  };
  unsafe { GetCursorInfo(&mut info) }.map_err(|error| error.to_string())?;
  let cursor = if info.flags.0 & CURSOR_SHOWING.0 != 0 {
    info.hCursor
  } else {
    cursors.arrow
  };
  Ok((
    f64::from(info.ptScreenPos.x),
    f64::from(info.ptScreenPos.y),
    cursor,
  ))
}

fn pressed(key: i32) -> bool {
  (unsafe { GetAsyncKeyState(key) }) < 0
}

fn run(stop: &AtomicBool, sink: &EventSink, ready: mpsc::Sender<Result<(), String>>) {
  let cursors = match StandardCursors::load() {
    Ok(cursors) => cursors,
    Err(error) => {
      let _ = ready.send(Err(error));
      return;
    }
  };
  let _ = ready.send(Ok(()));
  let buttons = [
    (VK_LBUTTON.0, CursorButton::Left),
    (VK_RBUTTON.0, CursorButton::Right),
    (VK_MBUTTON.0, CursorButton::Middle),
    (VK_XBUTTON1.0, CursorButton::Other(3)),
    (VK_XBUTTON2.0, CursorButton::Other(4)),
  ];
  let mut button_states = buttons.map(|(key, _)| pressed(i32::from(key)));
  let mut wrote_initial = false;
  let mut last_position = None;
  let mut last_cursor = None;
  let mut current_appearance = None;
  while !stop.load(Ordering::Acquire) {
    let kind = if wrote_initial {
      RawCursorEventKind::Move
    } else {
      RawCursorEventKind::Snapshot
    };
    let Ok((x, y, cursor)) = current_cursor(cursors) else {
      std::thread::sleep(POLL_INTERVAL);
      continue;
    };
    if last_cursor != Some(cursor) {
      current_appearance = Some(appearance(cursors, cursor));
    }
    let Some(cursor_appearance) = current_appearance.clone() else {
      continue;
    };
    let event = RawCursorEvent {
      appearance: cursor_appearance,
      at: Instant::now(),
      kind,
      x,
      y,
    };
    if !wrote_initial {
      wrote_initial = sink(event.clone());
    } else {
      let position = (event.x as i32, event.y as i32);
      if last_position != Some(position) {
        sink(event.clone());
      } else if last_cursor != Some(cursor) {
        let mut changed = event.clone();
        changed.kind = RawCursorEventKind::Appearance;
        sink(changed);
      }
      for (index, ((_, button), was_pressed)) in
        buttons.iter().zip(button_states.iter_mut()).enumerate()
      {
        let is_pressed = pressed(i32::from(buttons[index].0));
        if is_pressed != *was_pressed {
          let mut button_event = event.clone();
          button_event.kind = RawCursorEventKind::Button {
            button: *button,
            click_count: 1,
            state: if is_pressed {
              ButtonState::Down
            } else {
              ButtonState::Up
            },
          };
          sink(button_event);
          *was_pressed = is_pressed;
        }
      }
      last_position = Some(position);
    }
    last_cursor = Some(cursor);
    std::thread::sleep(POLL_INTERVAL);
  }
}

pub(super) fn start(stop: Arc<AtomicBool>, sink: EventSink) -> Result<JoinHandle<()>, String> {
  let (ready, did_start) = mpsc::channel();
  let worker_stop = Arc::clone(&stop);
  let worker = std::thread::Builder::new()
    .name("orbit-cursor-recorder".to_owned())
    .spawn(move || run(&worker_stop, &sink, ready))
    .map_err(|error| error.to_string())?;
  match did_start.recv_timeout(START_TIMEOUT) {
    Ok(Ok(())) => Ok(worker),
    Ok(Err(error)) => {
      let _ = worker.join();
      Err(error)
    }
    Err(_) => {
      stop.store(true, Ordering::Release);
      let _ = worker.join();
      Err("Cursor event recording did not start in time".to_owned())
    }
  }
}
