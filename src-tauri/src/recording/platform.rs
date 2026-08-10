// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::useless_transmute)]

//! Screen recording on macOS: ScreenCaptureKit into AVAssetWriter, hardware
//! H.264/HEVC encoding, no
//! intermediate files and no ffmpeg.
//!
//! # Who owns what
//!
//! `av::AssetWriter`, its input and the pixel buffer adaptor are not `Send`,
//! so they are created on, live on, and die on a single dedicated writer
//! thread that owns them for the whole recording. Nothing else ever touches
//! them. That thread's only input is one bounded channel.
//!
//! ScreenCaptureKit delivers frames on a dispatch queue. That callback does
//! the least work it possibly can - check the frame is a real one, retain its
//! pixel buffer, and hand it to the channel with `try_send`. It never blocks:
//! when the writer is behind, the channel is full and the frame is counted as
//! dropped rather than stalling the capture, which is what would make the
//! whole machine stutter.
//!
//! Pause, resume, stop and cancel travel down that same channel, so they are
//! ordered against the frames for free. There is no lock anywhere in the hot
//! path, and no state that two threads can see at once.

mod camera;
mod media;
mod output;
mod session;
mod startup;
mod writer;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::sync::OnceLock;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cidre::{
  arc, cg, cm, cv, define_obj_type, dispatch, objc, sc,
  sc::stream::{Output, OutputImpl},
};
use cpal::Stream;

use crate::capture_kit::{application_audio_filter, monitor_geometry, our_windows};

use super::encoding::FinalizeInfo;
use super::microphone::{
  Buffer as MicrophoneBuffer, Format as MicrophoneFormat, Source as MicrophoneSource,
};
use super::{CameraCaptureMode, CaptureStartupConfig, PrimaryCaptureSource};
#[cfg(test)]
use media::microphone_buffer_from_origin;
use media::{even, frame_status, time_to_ns, VideoEncoder};
use output::{AudioSample, CaptureStats, Command, Frame, ScreenOutput, ScreenOutputInner};
pub use session::CaptureSession;
use session::StreamObjects;
use writer::{Container, Writer, WriterConfig};

pub struct CaptureStart {
  pub first_frame: Receiver<Result<(), String>>,
  pub session: CaptureSession,
  pub source_scale_factor: f32,
}

pub fn begin_blocking(config: CaptureStartupConfig) -> Result<CaptureStart, String> {
  tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .map_err(|error| error.to_string())?
    .block_on(startup::begin(config))
}

/// How many frames ScreenCaptureKit may have in flight for us.
const STREAM_QUEUE_DEPTH: isize = 8;
/// How many frames may be waiting on the writer thread. Deeper than this only
/// buys latency: a backlog the writer cannot clear is a dropped frame either
/// way, and dropping it early keeps memory flat.
const FRAME_QUEUE_DEPTH: usize = 8;
/// Camera frames have a real cadence, unlike a screen stream that only emits
/// when pixels change. Give a temporarily busy camera encoder enough time to
/// drain into the bounded queue instead of punching visible holes in motion.
const CAMERA_ENCODER_WAIT: Duration = Duration::from_millis(100);
const CAMERA_ENCODER_POLL: Duration = Duration::from_millis(1);
const NANOS_PER_SEC: i64 = 1_000_000_000;
const NANOS_PER_MS: i64 = 1_000_000;
/// The long edge of the poster shipped to the export window.
const POSTER_MAX_EDGE: u32 = 640;
/// Finishing writes out the movie's index, which is fast but not instant.
const FINALIZE_TIMEOUT: Duration = Duration::from_secs(30);
/// How many frames in a row the writer may refuse before the user is told.
/// A handful of refusals is ordinary back-pressure; a run this long means the
/// recording is not going to recover on its own.
const REJECTION_STREAK_LIMIT: u64 = 60;
/// How hard the closing frame is pressed on a busy encoder, and how long
/// between tries. Half a second in total, which is not long enough to be felt
/// when stopping and is far longer than a real encoder ever needs.
const TAIL_APPEND_ATTEMPTS: u32 = 50;
const TAIL_APPEND_WAIT: Duration = Duration::from_millis(10);
const SYSTEM_AUDIO_SAMPLE_RATE: i64 = 48_000;
const SYSTEM_AUDIO_CHANNELS: i64 = 2;
const SYSTEM_AUDIO_BITRATE: i32 = 192_000;
const MICROPHONE_AUDIO_BITRATE: i32 = 128_000;
/// More than a second of audio at ScreenCaptureKit's usual 1024-frame buffer
/// size. In practice the first video frame arrives within one or two buffers;
/// the bound only prevents a broken display stream retaining audio forever.
const SYSTEM_AUDIO_PREROLL_LIMIT: usize = 64;
const MICROPHONE_PREROLL_LIMIT: usize = 64;

#[cfg(test)]
mod tests;
