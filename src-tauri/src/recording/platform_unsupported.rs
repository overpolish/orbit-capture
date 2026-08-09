// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! The capture pipeline on every platform that does not have one yet.
//!
//! Windows recording is a later slice, so this stands in for it: starting
//! fails with a plain sentence, the state machine reverts to idle and the UI
//! follows it. Nothing here can succeed, which is stated in the type - a
//! `CaptureSession` has no variants, so the compiler knows no session can
//! exist and the lifecycle code below it needs no platform branches at all.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use super::encoding::{FailureReport, FinalizeInfo};
use super::SystemAudioSelection;

pub enum CaptureSession {}

impl CaptureSession {
  pub fn pause(&self) {
    match *self {}
  }

  pub fn resume(&self) -> Result<(), String> {
    match *self {}
  }

  pub fn stop(self) -> Result<FinalizeInfo, String> {
    match self {}
  }

  pub fn cancel(self) {
    match self {}
  }
}

pub fn begin_blocking(
  monitor_id: u32,
  show_cursor: bool,
  system_audio: SystemAudioSelection,
  microphone_id: Option<String>,
  fps: u32,
  path: PathBuf,
  on_failure: FailureReport,
) -> Result<(CaptureSession, Receiver<Result<(), String>>), String> {
  let _ = (
    monitor_id,
    show_cursor,
    system_audio,
    microphone_id,
    fps,
    path,
    on_failure,
  );

  Err("Screen recording is not yet implemented on Windows".to_owned())
}
