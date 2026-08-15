// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Latest-frame-only camera confidence conversion for the Windows dock.

use std::{
  sync::{mpsc, Arc},
  thread::JoinHandle,
};

use crate::recording::monitor::RecordingMonitor;

const MAX_WIDTH: u32 = 48;
const MAX_HEIGHT: u32 = 30;

pub(super) struct CameraFrame {
  pub(super) flipped: bool,
  pub(super) height: u32,
  pub(super) rgba: Arc<Vec<u8>>,
  pub(super) width: u32,
}

pub(super) struct ConfidenceWorker {
  sender: Option<mpsc::SyncSender<CameraFrame>>,
  thread: Option<JoinHandle<()>>,
}

impl ConfidenceWorker {
  pub(super) fn spawn(monitor: Arc<RecordingMonitor>) -> Result<Self, String> {
    // A rendezvous channel has no stale-frame backlog: capture offers every
    // current frame, and drops it immediately while this worker is busy.
    let (sender, receiver) = mpsc::sync_channel::<CameraFrame>(0);
    let thread = std::thread::Builder::new()
      .name("orbit-camera-confidence-windows".to_owned())
      .spawn(move || {
        while let Ok(frame) = receiver.recv() {
          let (width, height, rgba) = thumbnail(&frame);
          monitor.send_camera(width, height, rgba);
        }
      })
      .map_err(|error| error.to_string())?;
    Ok(Self {
      sender: Some(sender),
      thread: Some(thread),
    })
  }

  pub(super) fn sender(&self) -> mpsc::SyncSender<CameraFrame> {
    self.sender.as_ref().expect("worker is active").clone()
  }

  pub(super) fn stop(mut self) {
    self.sender.take();
    if let Some(thread) = self.thread.take() {
      let _ = thread.join();
    }
  }
}

impl Drop for ConfidenceWorker {
  fn drop(&mut self) {
    self.sender.take();
    if let Some(thread) = self.thread.take() {
      let _ = thread.join();
    }
  }
}

fn thumbnail(frame: &CameraFrame) -> (u16, u16, Vec<u8>) {
  let scale = (f64::from(MAX_WIDTH) / f64::from(frame.width))
    .min(f64::from(MAX_HEIGHT) / f64::from(frame.height))
    .min(1.0);
  let width = (f64::from(frame.width) * scale).round().max(1.0) as u32;
  let height = (f64::from(frame.height) * scale).round().max(1.0) as u32;
  let mut output = vec![0_u8; width as usize * height as usize * 4];
  for y in 0..height {
    let source_y = (u64::from(y) * u64::from(frame.height) / u64::from(height)) as u32;
    for x in 0..width {
      let sampled_x = (u64::from(x) * u64::from(frame.width) / u64::from(width)) as u32;
      let source_x = if frame.flipped {
        frame.width - 1 - sampled_x
      } else {
        sampled_x
      };
      let source = ((source_y * frame.width + source_x) * 4) as usize;
      let target = ((y * width + x) * 4) as usize;
      output[target..target + 4].copy_from_slice(&frame.rgba[source..source + 4]);
    }
  }
  (width as u16, height as u16, output)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn downsamples_and_mirrors_the_latest_camera_frame() {
    let frame = CameraFrame {
      flipped: true,
      height: 1,
      rgba: Arc::new(vec![1, 2, 3, 4, 10, 20, 30, 40]),
      width: 2,
    };
    let (width, height, pixels) = thumbnail(&frame);
    assert_eq!((width, height), (2, 1));
    assert_eq!(pixels, vec![10, 20, 30, 40, 1, 2, 3, 4]);
  }
}
