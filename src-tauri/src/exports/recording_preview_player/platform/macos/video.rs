// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
  path::Path,
  sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{SyncSender, TrySendError},
    Arc,
  },
};

use cidre::{arc, av, cm, cv, ns};

use super::{
  composition::{cursor_rgba, still_overlay},
  cursor::{cursor_preview, CursorPreview},
  VideoFramePayload,
};
use crate::exports::cursor_effects::CursorOverlayCache;
use crate::exports::recording_preview_player::{
  video::{VideoFrame, PREVIEW_FPS},
  PlayerSources,
};
use crate::screenshots::CapturedImage;

unsafe extern "C" {
  fn screenwide_preview_reader_enable_random_access(output: *mut std::ffi::c_void);
  fn screenwide_preview_reader_reset_range(
    output: *mut std::ffi::c_void,
    start_milliseconds: i64,
    duration_milliseconds: i64,
  ) -> i32;
}

pub(super) struct NativeVideoReader {
  _reader: arc::R<av::AssetReader>,
  last_frame: Option<CapturedImage>,
  output: arc::R<av::AssetReaderTrackOutput>,
  pending: Option<arc::R<cm::SampleBuf>>,
  previous: Option<arc::R<cm::SampleBuf>>,
}

pub(super) fn open_asset(path: &Path) -> Result<arc::R<av::UrlAsset>, String> {
  let path_text = path
    .to_str()
    .ok_or_else(|| "The recording path is not valid UTF-8".to_owned())?;
  let url = ns::Url::with_fs_path_str(path_text, false);
  av::UrlAsset::with_url(&url, None)
    .ok_or_else(|| format!("AVFoundation could not open {}", path.display()))
}

fn output_settings(width: u32, height: u32) -> arc::R<ns::Dictionary<ns::String, ns::Id>> {
  let pixel_format = cv::PixelFormat::_32_BGRA.to_ns_number();
  let width = ns::Number::with_u32(width);
  let height = ns::Number::with_u32(height);
  ns::Dictionary::with_keys_values(
    &[
      cv::pixel_buffer_keys::pixel_format().as_ns(),
      cv::pixel_buffer_keys::width().as_ns(),
      cv::pixel_buffer_keys::height().as_ns(),
    ],
    &[
      pixel_format.as_id_ref(),
      width.as_id_ref(),
      height.as_id_ref(),
    ],
  )
}

impl NativeVideoReader {
  pub(super) fn open(
    asset: &av::UrlAsset,
    width: u32,
    height: u32,
    start_ms: u64,
    duration_ms: u64,
  ) -> Result<Self, String> {
    let tracks =
      tauri::async_runtime::block_on(asset.load_tracks_with_media_type(av::MediaType::video()))
        .map_err(|error| error.to_string())?;
    let track = tracks
      .get(0)
      .map_err(|_| "The recording has no video track".to_owned())?;
    let settings = output_settings(width, height);
    let mut output = av::AssetReaderTrackOutput::with_track(&track, Some(&settings))
      .map_err(|error| error.to_string())?;
    output.set_always_copies_sample_data(false);
    unsafe {
      screenwide_preview_reader_enable_random_access(output.as_ptr().cast());
    }
    let mut reader = av::AssetReader::with_asset(asset).map_err(|error| error.to_string())?;
    reader
      .set_time_range(cm::TimeRange {
        start: cm::Time::new(start_ms as i64, 1_000),
        duration: cm::Time::new(duration_ms.saturating_sub(start_ms).max(1) as i64, 1_000),
      })
      .map_err(|error| error.to_string())?;
    reader
      .add_output(&output)
      .map_err(|error| error.to_string())?;
    if !reader.start_reading().map_err(|error| error.to_string())? {
      return Err(reader.error().map_or_else(
        || "AVFoundation could not start preview playback".to_owned(),
        |error| error.to_string(),
      ));
    }
    Ok(Self {
      _reader: reader,
      last_frame: None,
      output,
      pending: None,
      previous: None,
    })
  }

  /// Repositions the existing AVFoundation decode pipeline. Random-access
  /// outputs keep their decoder and asset I/O state alive across scrubs, which
  /// avoids paying AVAssetReader construction cost for every backward jump.
  pub(super) fn reset(&mut self, start_ms: u64, duration_ms: u64) -> Result<(), String> {
    self.pending = None;
    self.previous = None;
    self.last_frame = None;
    let reset = unsafe {
      screenwide_preview_reader_reset_range(
        self.output.as_ptr().cast(),
        start_ms as i64,
        duration_ms.saturating_sub(start_ms).max(1) as i64,
      )
    };
    if reset == 0 {
      Err("AVFoundation could not reposition the preview decoder".to_owned())
    } else {
      Ok(())
    }
  }

  pub(super) fn frame_at(&mut self, target_ms: u64) -> Result<Option<CapturedImage>, String> {
    loop {
      if self.pending.is_none() {
        self.pending = self
          .output
          .next_sample_buf()
          .map_err(|error| error.to_string())?;
      }
      let Some(sample) = self.pending.as_ref() else {
        // The range is exhausted: the newest sample skipped past (retained
        // undecoded below) is still the correct still for a seek at or past
        // the end of the track.
        if let Some(previous) = self.previous.take() {
          return Self::converted(previous).map(|frame| {
            self.last_frame = Some(frame.clone());
            Some(frame)
          });
        }
        return Ok(self.last_frame.clone());
      };
      let pts_ms = (sample.pts().as_secs().max(0.0) * 1_000.0).round() as u64;
      if pts_ms.saturating_add(2) < target_ms {
        self.previous = self.pending.take();
        continue;
      }
      let sample = self.pending.take().expect("the pending sample exists");
      self.previous = None;
      let frame = Self::converted(sample)?;
      self.last_frame = Some(frame.clone());
      return Ok(Some(frame));
    }
  }

  fn converted(mut sample: arc::R<cm::SampleBuf>) -> Result<CapturedImage, String> {
    {
      let pixel_buffer = sample
        .image_buf_mut()
        .ok_or_else(|| "AVFoundation returned a video sample without pixels".to_owned())?;
      let width = pixel_buffer.width();
      let height = pixel_buffer.height();
      let stride = pixel_buffer.bytes_per_row();
      let flags = cv::pixel_buffer::LockFlags::READ_ONLY;
      unsafe { pixel_buffer.lock_base_addr(flags) }
        .result()
        .map_err(|error| error.to_string())?;
      let base = unsafe { pixel_buffer.base_address() } as *const u8;
      if base.is_null() {
        unsafe { pixel_buffer.unlock_lock_base_addr(flags) };
        return Err("AVFoundation returned an empty video frame".to_owned());
      }
      let mut rgba = vec![0_u8; width * height * 4];
      for row in 0..height {
        let source = unsafe { std::slice::from_raw_parts(base.add(row * stride), width * 4) };
        let target = &mut rgba[row * width * 4..(row + 1) * width * 4];
        for (source, target) in source.chunks_exact(4).zip(target.chunks_exact_mut(4)) {
          target[0] = source[2];
          target[1] = source[1];
          target[2] = source[0];
          target[3] = source[3];
        }
      }
      unsafe { pixel_buffer.unlock_lock_base_addr(flags) };
      Ok(CapturedImage {
        height: height as u32,
        rgba,
        width: width as u32,
      })
    }
  }
}

fn scaled_dimension(source: u32, factor: f64) -> u32 {
  ((f64::from(source) * factor.clamp(0.0, 1.0))
    .round()
    .max(2.0) as u32)
    .min(source)
}

pub(super) fn spawn(
  sources: &PlayerSources,
  playback_factors: &[f64],
  start_ms: u64,
  cancelled: Arc<AtomicBool>,
  sender: SyncSender<VideoFrame>,
) -> Result<std::thread::JoinHandle<()>, String> {
  // Decoding and composing at the on-screen pane size keeps playback frames
  // pixel-identical to paused frames and avoids Core Animation minifying a
  // full-resolution drawable, which visibly brightens thin glyphs.
  let screen_factor = playback_factors.first().copied().unwrap_or(1.0);
  let camera_factor = playback_factors.get(1).copied().unwrap_or(1.0);
  let screen_pane = &sources.playback_layout.panes[0];
  let mut screen = NativeVideoReader::open(
    &*open_asset(&sources.screen_path)?,
    scaled_dimension(screen_pane.source_width, screen_factor),
    scaled_dimension(screen_pane.source_height, screen_factor),
    start_ms,
    sources.duration_ms,
  )?;
  let mut camera = match (
    sources.camera_path.as_deref(),
    sources.camera_duration_ms,
    sources.playback_layout.panes.get(1),
  ) {
    (Some(path), Some(duration_ms), Some(pane)) => Some(NativeVideoReader::open(
      &*open_asset(path)?,
      scaled_dimension(pane.source_width, camera_factor),
      scaled_dimension(pane.source_height, camera_factor),
      start_ms.min(duration_ms.saturating_sub(1)),
      duration_ms,
    )?),
    _ => None,
  };
  let cursor = sources.cursor.clone();
  let cursor_settings = Arc::clone(&sources.cursor_settings);
  let composition_settings = sources.composition_settings.clone();
  let duration_ms = sources.duration_ms;
  let cursor_output = (
    sources.playback_layout.panes[0].source_width,
    sources.playback_layout.panes[0].source_height,
  );

  std::thread::Builder::new()
    .name("recording-preview-video-native".to_owned())
    .spawn(move || {
      let mut index = 0;
      let mut cursor_cache = CursorOverlayCache::new();
      while !cancelled.load(Ordering::Acquire) {
        let target_ms = start_ms.saturating_add(index * 1_000 / PREVIEW_FPS);
        if target_ms >= duration_ms {
          break;
        }
        let cursor_settings = cursor_settings
          .read()
          .map(|settings| *settings)
          .unwrap_or_default();
        let composition = composition_settings
          .as_ref()
          .expect("the recording player always has composition settings")
          .read()
          .map(|settings| settings.clone())
          .unwrap_or_else(|poisoned| poisoned.into_inner().clone());
        let raw_screen = match screen.frame_at(target_ms) {
          Ok(Some(frame)) => frame,
          Ok(None) | Err(_) => break,
        };
        let raw_camera = match camera.as_mut() {
          Some(reader) => match reader.frame_at(target_ms) {
            Ok(Some(frame)) => Some(frame),
            Ok(None) => None,
            Err(_) => break,
          },
          None => None,
        };
        let cursor_frame: Option<CursorPreview> = cursor_preview(
          cursor.as_deref(),
          target_ms,
          cursor_settings,
          cursor_output,
          &mut cursor_cache,
        )
        .unwrap_or_default();
        let cursor_pixels = cursor_frame
          .as_ref()
          .and_then(|cursor| cursor_rgba(cursor).ok());
        let screen_output =
          super::still_decode::scaled_output(&composition.recording_output.primary, screen_factor);
        let (cursor_image, overlay) = match still_overlay(
          &raw_screen,
          &screen_output,
          cursor_frame.as_ref().zip(cursor_pixels),
          composition
            .bake_camera
            .then_some(raw_camera.as_ref())
            .flatten(),
          composition
            .bake_camera
            .then_some(composition.camera_overlay),
          composition.recording_output.camera.drop_shadow,
          composition.recording_output.camera_on_top,
        ) {
          Ok(value) => value,
          Err(_) => break,
        };
        let camera_output =
          super::still_decode::scaled_output(&composition.recording_output.camera, camera_factor);
        let mut frame = VideoFrame {
          timestamp_ms: target_ms,
          payload: VideoFramePayload::Native {
            screen: raw_screen,
            camera: raw_camera,
            screen_output,
            camera_output,
            cursor_image,
            overlay,
            bake_camera: composition.bake_camera,
            seconds: target_ms as f64 / 1_000.0,
            clip_cursor_at_video_edge: cursor_settings.clip_at_video_edge,
          },
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
