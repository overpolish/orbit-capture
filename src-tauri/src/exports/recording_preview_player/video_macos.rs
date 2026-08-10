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

use cidre::{arc, av, cm, cv, ns, objc::ar_pool, vt};

use super::{
  layout::PreviewPane,
  still_macos::jpeg,
  video::{VideoFrame, VideoFramePayload, PREVIEW_FPS},
  PlayerSources,
};

struct NativeVideoReader {
  _reader: arc::R<av::AssetReader>,
  last_frame: Option<Vec<u8>>,
  output: arc::R<av::AssetReaderTrackOutput>,
  pending: Option<arc::R<cm::SampleBuf>>,
}

fn output_settings(pane: &PreviewPane) -> arc::R<ns::Dictionary<ns::String, ns::Id>> {
  let pixel_format = cv::PixelFormat::_32_BGRA.to_ns_number();
  let width = ns::Number::with_u32(pane.width);
  let height = ns::Number::with_u32(pane.height);
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
  fn open(
    path: &Path,
    pane: &PreviewPane,
    start_ms: u64,
    duration_ms: u64,
  ) -> Result<Self, String> {
    let path_text = path
      .to_str()
      .ok_or_else(|| "The recording path is not valid UTF-8".to_owned())?;
    let url = ns::Url::with_fs_path_str(path_text, false);
    let asset = av::UrlAsset::with_url(&url, None)
      .ok_or_else(|| format!("AVFoundation could not open {}", path.display()))?;
    let tracks =
      tauri::async_runtime::block_on(asset.load_tracks_with_media_type(av::MediaType::video()))
        .map_err(|error| error.to_string())?;
    let track = tracks
      .get(0)
      .map_err(|_| format!("{} has no video track", path.display()))?;
    let settings = output_settings(pane);
    let mut output = av::AssetReaderTrackOutput::with_track(&track, Some(&settings))
      .map_err(|error| error.to_string())?;
    output.set_always_copies_sample_data(false);
    let mut reader = av::AssetReader::with_asset(&asset).map_err(|error| error.to_string())?;
    reader
      .set_time_range(cm::TimeRange {
        start: cm::Time::new(start_ms as i64, 1_000),
        duration: cm::Time::new(duration_ms.saturating_sub(start_ms) as i64, 1_000),
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
    })
  }

  fn frame_at(&mut self, target_ms: u64) -> Result<Option<Vec<u8>>, String> {
    loop {
      if self.pending.is_none() {
        self.pending = self
          .output
          .next_sample_buf()
          .map_err(|error| error.to_string())?;
      }
      let Some(sample) = self.pending.as_ref() else {
        return Ok(self.last_frame.clone());
      };
      let pts_ms = (sample.pts().as_secs().max(0.0) * 1_000.0).round() as u64;
      if pts_ms.saturating_add(2) < target_ms {
        self.pending = None;
        continue;
      }
      let sample = self.pending.take().expect("the pending sample exists");
      let encoded = ar_pool(|| {
        let pixel_buffer = sample
          .image_buf()
          .ok_or_else(|| "AVFoundation returned a video sample without pixels".to_owned())?;
        let image =
          vt::cg_image_from_cv_pixel_buf(pixel_buffer, None).map_err(|error| error.to_string())?;
        jpeg(&image)
      })?;
      self.last_frame = Some(encoded.clone());
      return Ok(Some(encoded));
    }
  }
}

pub(super) fn spawn(
  sources: &PlayerSources,
  start_ms: u64,
  cancelled: Arc<AtomicBool>,
  sender: SyncSender<VideoFrame>,
) -> Result<std::thread::JoinHandle<()>, String> {
  let mut screen = NativeVideoReader::open(
    &sources.screen_path,
    &sources.playback_layout.panes[0],
    start_ms,
    sources.duration_ms,
  )?;
  let mut camera = match (
    sources.camera_path.as_deref(),
    sources.playback_layout.panes.get(1),
  ) {
    (Some(path), Some(pane)) => Some(NativeVideoReader::open(
      path,
      pane,
      start_ms,
      sources.duration_ms,
    )?),
    _ => None,
  };

  std::thread::Builder::new()
    .name("recording-preview-video-native".to_owned())
    .spawn(move || {
      let mut index = 0;
      while !cancelled.load(Ordering::Acquire) {
        let target_ms = start_ms.saturating_add(index * 1_000 / PREVIEW_FPS);
        let screen_frame = match screen.frame_at(target_ms) {
          Ok(Some(frame)) => frame,
          Ok(None) | Err(_) => break,
        };
        let camera_frame = match camera.as_mut() {
          Some(reader) => match reader.frame_at(target_ms) {
            Ok(frame) => frame,
            Err(_) => break,
          },
          None => None,
        };
        let mut frame = VideoFrame {
          index,
          payload: VideoFramePayload::Native {
            screen: screen_frame,
            camera: camera_frame,
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
