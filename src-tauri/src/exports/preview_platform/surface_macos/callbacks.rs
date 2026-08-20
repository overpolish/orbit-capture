// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::super::{
  SelectionCallback, SelectionGestureCallback, SelectionGestureOperation, SelectionGesturePhase,
  TransformCallback,
};
use super::ffi::screenwide_preview_surface_release_context_on_main;

pub(super) unsafe extern "C" fn release_boxed_callback<T>(context: *mut std::ffi::c_void) {
  drop(unsafe { Box::from_raw(context.cast::<T>()) });
}

/// Frees a callback box on the main thread instead of here. The native
/// callback setters apply asynchronously (a synchronous hop deadlocks against
/// the player mutex), so when this thread returns from clearing or replacing
/// a callback the main thread may still hold the old context pointer. The
/// free is queued behind that clear, which is the last block that can read it.
pub(super) fn release_callback_on_main<T>(callback: Option<Box<T>>) {
  if let Some(callback) = callback {
    unsafe {
      screenwide_preview_surface_release_context_on_main(
        release_boxed_callback::<T>,
        Box::into_raw(callback).cast::<std::ffi::c_void>(),
      );
    }
  }
}

unsafe extern "C" fn run_boxed_closure(context: *mut std::ffi::c_void) {
  let work = unsafe { Box::from_raw(context.cast::<Box<dyn FnOnce() + Send>>()) };
  work();
}

/// Runs `work` on the main thread *behind* every block the surface has already
/// queued there. Tauri's `run_on_main_thread` goes through the event-loop
/// proxy, a different queue from the dispatch main queue the native layout
/// blocks use, so it gives no ordering against them: a caller waiting for a
/// layout block to run can spin through its whole retry budget on the proxy
/// before the block gets a turn, which is what happens whenever another
/// surface keeps the main queue busy. This hops through the main queue itself.
pub(crate) fn run_on_main_queue(work: Box<dyn FnOnce() + Send>) {
  let context = Box::into_raw(Box::new(work)).cast::<std::ffi::c_void>();
  unsafe {
    screenwide_preview_surface_release_context_on_main(run_boxed_closure, context);
  }
}

pub(super) unsafe extern "C" fn transform_callback(
  zoom_percent: f64,
  context: *mut std::ffi::c_void,
) {
  if let Some(callback) = (context as *mut TransformCallback).as_mut() {
    callback(zoom_percent);
  }
}

pub(super) unsafe extern "C" fn selection_callback(
  pane_index: i32,
  context: *mut std::ffi::c_void,
) {
  if let Some(callback) = (context as *mut SelectionCallback).as_mut() {
    callback(u32::try_from(pane_index).ok());
  }
}

pub(super) unsafe extern "C" fn selection_gesture_callback(
  phase: u32,
  pane_index: u32,
  operation: u32,
  edges: u32,
  scale: f64,
  delta_x: f64,
  delta_y: f64,
  context: *mut std::ffi::c_void,
) {
  if let Some(callback) = (context as *mut SelectionGestureCallback).as_mut() {
    let phase = match phase {
      0 => SelectionGesturePhase::Begin,
      1 => SelectionGesturePhase::Update,
      2 => SelectionGesturePhase::End,
      3 => SelectionGesturePhase::Cancel,
      _ => return,
    };
    let operation = match operation {
      0 => SelectionGestureOperation::Move,
      1 => SelectionGestureOperation::Resize,
      2 => SelectionGestureOperation::Radius,
      3 => SelectionGestureOperation::FrameResize,
      4 => SelectionGestureOperation::FrameRadius,
      5 => SelectionGestureOperation::CropMove,
      6 => SelectionGestureOperation::CropResize,
      _ => return,
    };
    callback(phase, pane_index, operation, edges, scale, delta_x, delta_y);
  }
}
