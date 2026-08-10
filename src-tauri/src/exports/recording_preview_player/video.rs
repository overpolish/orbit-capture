// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#[cfg(not(target_os = "macos"))]
use std::{
  ffi::OsString,
  io::{BufReader, Read},
  process::{Child, Command, Stdio},
  sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{SyncSender, TrySendError},
    Arc, Mutex,
  },
};

#[cfg(not(target_os = "macos"))]
use super::PlayerSources;
#[cfg(not(target_os = "macos"))]
use crate::exports::media_preview;

pub(super) const PREVIEW_FPS: u64 = 30;

pub(super) struct VideoFrame {
  pub index: u64,
  pub payload: VideoFramePayload,
}

pub(super) enum VideoFramePayload {
  #[cfg(not(target_os = "macos"))]
  Composite(Vec<u8>),
  #[cfg(target_os = "macos")]
  Native {
    screen: Vec<u8>,
    camera: Option<Vec<u8>>,
  },
}

#[cfg(not(target_os = "macos"))]
fn seconds(milliseconds: u64) -> String {
  format!("{}.{:03}", milliseconds / 1_000, milliseconds % 1_000)
}

#[cfg(not(target_os = "macos"))]
fn filter(sources: &PlayerSources) -> String {
  let screen = &sources.playback_layout.panes[0];
  if sources.camera_path.is_none() {
    return format!(
      "[0:v:0]setpts=PTS-STARTPTS,scale={}:{}:flags=fast_bilinear,fps={PREVIEW_FPS}[preview]",
      screen.width, screen.height
    );
  }
  let camera = &sources.playback_layout.panes[1];
  format!(
    "[0:v:0]setpts=PTS-STARTPTS,scale={}:{}:flags=fast_bilinear[screen];[1:v:0]setpts=PTS-STARTPTS,scale={}:{}:flags=fast_bilinear[camera];[screen][camera]hstack=inputs=2:shortest=0,fps={PREVIEW_FPS}[preview]",
    screen.width, screen.height, camera.width, camera.height
  )
}

#[cfg(not(target_os = "macos"))]
fn args(sources: &PlayerSources, start_ms: u64, still: bool) -> Vec<OsString> {
  let start = seconds(start_ms);
  let mut args: Vec<OsString> = [
    "-hide_banner",
    "-loglevel",
    "error",
    "-nostdin",
    "-hwaccel",
    "auto",
    "-threads",
    "4",
    "-ss",
  ]
  .map(OsString::from)
  .into();
  args.push(start.clone().into());
  args.push("-i".into());
  args.push(sources.screen_path.as_os_str().to_owned());
  if let Some(camera) = &sources.camera_path {
    args.extend(["-hwaccel", "auto", "-threads", "4", "-ss"].map(OsString::from));
    args.push(start.into());
    args.push("-i".into());
    args.push(camera.as_os_str().to_owned());
  }
  args.extend(["-filter_complex".into(), filter(sources).into()]);
  args.extend(["-map", "[preview]", "-an"].map(OsString::from));
  if still {
    args.extend(["-frames:v", "1"].map(OsString::from));
  }
  args.extend(
    [
      "-c:v",
      "mjpeg",
      "-q:v",
      "4",
      "-pix_fmt",
      "yuvj420p",
      "-f",
      "image2pipe",
      "pipe:1",
    ]
    .map(OsString::from),
  );
  args
}

#[cfg(not(target_os = "macos"))]
fn next_jpeg(
  reader: &mut BufReader<impl Read>,
  pending: &mut Vec<u8>,
) -> std::io::Result<Option<Vec<u8>>> {
  loop {
    if let Some(end) = pending
      .windows(2)
      .position(|pair| pair == [0xff, 0xd9])
      .map(|index| index + 2)
    {
      let remainder = pending.split_off(end);
      let frame = std::mem::replace(pending, remainder);
      return Ok(Some(frame));
    }
    let mut chunk = [0_u8; 64 * 1_024];
    let count = reader.read(&mut chunk)?;
    if count == 0 {
      return Ok(None);
    }
    pending.extend_from_slice(&chunk[..count]);
    if let Some(start) = pending.windows(2).position(|pair| pair == [0xff, 0xd8]) {
      if start > 0 {
        pending.drain(..start);
      }
    }
  }
}

#[cfg(not(target_os = "macos"))]
pub(super) fn spawn(
  sources: &PlayerSources,
  start_ms: u64,
  still: bool,
  cancelled: Arc<AtomicBool>,
  child: Arc<Mutex<Option<Child>>>,
  sender: SyncSender<VideoFrame>,
) -> Result<std::thread::JoinHandle<()>, String> {
  let mut process = Command::new(media_preview::ffmpeg_path());
  process
    .args(args(sources, start_ms, still))
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
  let mut process = process
    .spawn()
    .map_err(|error| format!("FFmpeg could not start preview video: {error}"))?;
  let stdout = process
    .stdout
    .take()
    .ok_or_else(|| "FFmpeg did not expose preview frames".to_owned())?;
  *child
    .lock()
    .map_err(|_| "The preview video process is unavailable".to_owned())? = Some(process);

  std::thread::Builder::new()
    .name("recording-preview-video".to_owned())
    .spawn(move || {
      let mut reader = BufReader::new(stdout);
      let mut pending = Vec::new();
      let mut index = 0;
      while !cancelled.load(Ordering::Acquire) {
        let bytes = match next_jpeg(&mut reader, &mut pending) {
          Ok(Some(bytes)) => bytes,
          Ok(None) | Err(_) => break,
        };
        let mut frame = VideoFrame {
          index,
          payload: VideoFramePayload::Composite(bytes),
        };
        loop {
          match sender.try_send(frame) {
            Ok(()) => break,
            Err(TrySendError::Full(returned)) => {
              if cancelled.load(Ordering::Acquire) {
                return;
              }
              frame = returned;
              std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Err(TrySendError::Disconnected(_)) => return,
          }
        }
        index += 1;
      }
    })
    .map_err(|error| error.to_string())
}

#[cfg(all(test, not(target_os = "macos")))]
mod tests {
  use super::*;

  #[test]
  fn separates_consecutive_jpeg_images() {
    let bytes = [0xff, 0xd8, 1, 0xff, 0xd9, 0xff, 0xd8, 2, 0xff, 0xd9];
    let mut reader = BufReader::new(bytes.as_slice());
    let mut pending = Vec::new();
    assert_eq!(
      next_jpeg(&mut reader, &mut pending).unwrap(),
      Some(vec![0xff, 0xd8, 1, 0xff, 0xd9])
    );
    assert_eq!(
      next_jpeg(&mut reader, &mut pending).unwrap(),
      Some(vec![0xff, 0xd8, 2, 0xff, 0xd9])
    );
  }
}
