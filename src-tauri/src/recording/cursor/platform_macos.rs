// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::Hasher;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoop};
use core_graphics::event::{
  CGEvent, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
  CallbackResult, EventField,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use objc2::rc::Retained;
use objc2_app_kit::{NSCursor, NSImage};

use super::{
  ButtonState, CursorAppearance, CursorButton, CursorStyle, EventSink, RawCursorEvent,
  RawCursorEventKind,
};

const RUN_LOOP_POLL: Duration = Duration::from_millis(50);
const START_TIMEOUT: Duration = Duration::from_secs(2);
const MOVEMENT_INTERVAL: Duration = Duration::from_micros(7_500);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CursorFingerprint {
  bytes: usize,
  hash: u64,
  height: u64,
  hotspot_x: u64,
  hotspot_y: u64,
  width: u64,
}

fn fingerprint(cursor: &NSCursor) -> CursorFingerprint {
  let hotspot = cursor.hotSpot();
  let image = cursor.image();
  let size = image.size();
  let data = image.TIFFRepresentation();
  let mut hasher = DefaultHasher::new();
  if let Some(data) = &data {
    // This fingerprint exists only long enough to map AppKit's cursor to a
    // semantic label. Cursor image bytes never enter the recording sidecar.
    // SAFETY: `data` is immutable and retained for the lifetime of this slice.
    hasher.write(unsafe { data.as_bytes_unchecked() });
  }
  CursorFingerprint {
    bytes: data.as_ref().map_or(0, |data| data.len()),
    hash: hasher.finish(),
    height: size.height.to_bits(),
    hotspot_x: hotspot.x.to_bits(),
    hotspot_y: hotspot.y.to_bits(),
    width: size.width.to_bits(),
  }
}

struct CursorCatalog {
  cache: RefCell<HashMap<usize, (Retained<NSImage>, CursorAppearance)>>,
  entries: Vec<(CursorStyle, CursorFingerprint)>,
}

impl CursorCatalog {
  #[allow(deprecated)]
  fn new() -> Self {
    let cursors = vec![
      (CursorStyle::Arrow, NSCursor::arrowCursor()),
      (CursorStyle::IBeam, NSCursor::IBeamCursor()),
      (
        CursorStyle::VerticalIBeam,
        NSCursor::IBeamCursorForVerticalLayout(),
      ),
      (CursorStyle::Crosshair, NSCursor::crosshairCursor()),
      (CursorStyle::PointingHand, NSCursor::pointingHandCursor()),
      (CursorStyle::OpenHand, NSCursor::openHandCursor()),
      (CursorStyle::ClosedHand, NSCursor::closedHandCursor()),
      (
        CursorStyle::ResizeHorizontal,
        NSCursor::resizeLeftRightCursor(),
      ),
      (CursorStyle::ResizeVertical, NSCursor::resizeUpDownCursor()),
      (CursorStyle::ContextMenu, NSCursor::contextualMenuCursor()),
      (CursorStyle::DragCopy, NSCursor::dragCopyCursor()),
      (CursorStyle::DragLink, NSCursor::dragLinkCursor()),
      (
        CursorStyle::DisappearingItem,
        NSCursor::disappearingItemCursor(),
      ),
      (
        CursorStyle::NotAllowed,
        NSCursor::operationNotAllowedCursor(),
      ),
      (CursorStyle::ZoomIn, NSCursor::zoomInCursor()),
      (CursorStyle::ZoomOut, NSCursor::zoomOutCursor()),
    ];
    Self {
      cache: RefCell::new(HashMap::new()),
      entries: cursors
        .into_iter()
        .map(|(style, cursor)| (style, fingerprint(&cursor)))
        .collect(),
    }
  }

  #[allow(deprecated)]
  fn current(&self) -> CursorAppearance {
    let Some(cursor) = NSCursor::currentSystemCursor() else {
      return CursorAppearance {
        height: 0.0,
        hotspot_x: 0.0,
        hotspot_y: 0.0,
        style: CursorStyle::Custom,
        width: 0.0,
      };
    };
    let hotspot = cursor.hotSpot();
    let image = cursor.image();
    let size = image.size();
    let image_key = Retained::as_ptr(&image) as usize;
    if let Some((_, appearance)) = self.cache.borrow().get(&image_key) {
      return appearance.clone();
    }
    let current = fingerprint(&cursor);
    let style = self
      .entries
      .iter()
      .find_map(|(style, candidate)| (*candidate == current).then_some(*style))
      .unwrap_or(CursorStyle::Custom);
    let appearance = CursorAppearance {
      height: size.height,
      hotspot_x: hotspot.x,
      hotspot_y: hotspot.y,
      style,
      width: size.width,
    };
    self
      .cache
      .borrow_mut()
      .insert(image_key, (image, appearance.clone()));
    appearance
  }
}

fn button(event_type: CGEventType, event: &CGEvent) -> Option<(CursorButton, ButtonState)> {
  let state = match event_type {
    CGEventType::LeftMouseDown | CGEventType::RightMouseDown | CGEventType::OtherMouseDown => {
      ButtonState::Down
    }
    CGEventType::LeftMouseUp | CGEventType::RightMouseUp | CGEventType::OtherMouseUp => {
      ButtonState::Up
    }
    _ => return None,
  };
  let number = event.get_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER);
  let button = match number {
    0 => CursorButton::Left,
    1 => CursorButton::Right,
    2 => CursorButton::Middle,
    other => CursorButton::Other(u8::try_from(other).unwrap_or(u8::MAX)),
  };
  Some((button, state))
}

fn raw_event(
  catalog: &CursorCatalog,
  event_type: CGEventType,
  event: &CGEvent,
  at: Instant,
) -> RawCursorEvent {
  let point = event.location();
  let kind = button(event_type, event).map_or(RawCursorEventKind::Move, |(button, state)| {
    let click_count = event
      .get_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE)
      .clamp(0, i64::from(u8::MAX)) as u8;
    RawCursorEventKind::Button {
      button,
      click_count,
      state,
    }
  });
  RawCursorEvent {
    appearance: catalog.current(),
    at,
    kind,
    x: point.x,
    y: point.y,
  }
}

fn current_event(catalog: &CursorCatalog, kind: RawCursorEventKind) -> Option<RawCursorEvent> {
  let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).ok()?;
  let event = CGEvent::new(source).ok()?;
  let mut event = raw_event(catalog, CGEventType::MouseMoved, &event, Instant::now());
  event.kind = kind;
  Some(event)
}

fn is_motion(event_type: CGEventType) -> bool {
  matches!(
    event_type,
    CGEventType::MouseMoved
      | CGEventType::LeftMouseDragged
      | CGEventType::RightMouseDragged
      | CGEventType::OtherMouseDragged
  )
}

fn run(stop: &AtomicBool, sink: &EventSink, ready: mpsc::Sender<Result<(), String>>) {
  let catalog = CursorCatalog::new();
  let ready = RefCell::new(Some(ready));
  let last_motion = Cell::new(None::<Instant>);
  let result = CGEventTap::with_enabled(
    CGEventTapLocation::HID,
    CGEventTapPlacement::HeadInsertEventTap,
    CGEventTapOptions::ListenOnly,
    vec![
      CGEventType::MouseMoved,
      CGEventType::LeftMouseDragged,
      CGEventType::RightMouseDragged,
      CGEventType::OtherMouseDragged,
      CGEventType::LeftMouseDown,
      CGEventType::LeftMouseUp,
      CGEventType::RightMouseDown,
      CGEventType::RightMouseUp,
      CGEventType::OtherMouseDown,
      CGEventType::OtherMouseUp,
    ],
    |_, event_type, event| {
      let at = Instant::now();
      if is_motion(event_type)
        && last_motion
          .get()
          .is_some_and(|last| at.saturating_duration_since(last) < MOVEMENT_INTERVAL)
      {
        return CallbackResult::Keep;
      }
      if is_motion(event_type) {
        last_motion.set(Some(at));
      }
      sink(raw_event(&catalog, event_type, event, at));
      CallbackResult::Keep
    },
    || {
      if let Some(ready) = ready.borrow_mut().take() {
        let _ = ready.send(Ok(()));
      }
      let mut wrote_initial = false;
      while !stop.load(Ordering::Acquire) {
        if !wrote_initial {
          wrote_initial =
            current_event(&catalog, RawCursorEventKind::Snapshot).is_some_and(|event| sink(event));
        }
        // SAFETY: Core Foundation owns this process-global constant for the
        // lifetime of the process.
        unsafe {
          CFRunLoop::run_in_mode(kCFRunLoopDefaultMode, RUN_LOOP_POLL, false);
        }
        if wrote_initial {
          if let Some(event) = current_event(&catalog, RawCursorEventKind::Appearance) {
            sink(event);
          }
        }
      }
    },
  );
  if result.is_err() {
    if let Some(ready) = ready.borrow_mut().take() {
      let _ = ready.send(Err(
        "Could not listen for cursor events; Accessibility access is required".to_owned(),
      ));
    }
  }
}

pub(super) fn start(stop: Arc<AtomicBool>, sink: EventSink) -> Result<JoinHandle<()>, String> {
  let (ready, did_start) = mpsc::channel();
  let worker_stop = Arc::clone(&stop);
  let worker = std::thread::Builder::new()
    .name("screenwide-cursor-recorder".to_owned())
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
