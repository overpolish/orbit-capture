// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{path::Path, sync::mpsc, thread::JoinHandle};

use cidre::{av, cf, cg, cm, ns, ut};
use tauri::ipc::{Channel, InvokeResponseBody};

use super::{layout::PreviewPane, PlayerSources, RecordingPreviewPlayerEvent};

const NATIVE_FRAME_MARKER: u32 = u32::from_le_bytes(*b"OCPF");

enum DecoderCommand {
  Seek {
    full_resolution: bool,
    position_ms: u64,
    request_id: u64,
  },
  Stop,
}

pub(super) struct NativeStillDecoder {
  sender: mpsc::Sender<DecoderCommand>,
  thread: Option<JoinHandle<()>>,
}

fn image_generator(path: &Path) -> Result<cidre::arc::R<av::AssetImageGenerator>, String> {
  let path_text = path
    .to_str()
    .ok_or_else(|| "The recording path is not valid UTF-8".to_owned())?;
  let url = ns::Url::with_fs_path_str(path_text, false);
  let asset = av::UrlAsset::with_url(&url, None)
    .ok_or_else(|| format!("AVFoundation could not open {}", path.display()))?;
  let mut generator = av::AssetImageGenerator::with_asset(&asset);
  generator.set_applies_preferred_track_transform(true);
  let tolerance = cm::Time::new(1, 60);
  generator.set_requested_time_tolerance_before(tolerance);
  generator.set_requested_time_tolerance_after(tolerance);
  Ok(generator)
}

fn set_size(generator: &mut av::AssetImageGenerator, pane: &PreviewPane, full_resolution: bool) {
  let (width, height) = if full_resolution {
    (pane.source_width, pane.source_height)
  } else {
    (pane.width, pane.height)
  };
  generator.set_max_size(cg::Size {
    width: f64::from(width),
    height: f64::from(height),
  });
}

pub(super) fn jpeg(image: &cg::Image) -> Result<Vec<u8>, String> {
  // CFDataCreateMutable treats a non-zero capacity as a hard maximum rather
  // than an initial allocation. Preview JPEGs can easily exceed 128 KiB, so
  // use zero to allow ImageIO to grow the destination as it encodes.
  let mut data = cf::DataMut::with_capacity(0);
  let jpeg_type = ut::Type::jpeg().id();
  let mut destination = cg::ImageDst::with_data(&mut data, jpeg_type.as_cf(), 1)
    .ok_or_else(|| "Core Graphics could not create a preview image".to_owned())?;
  destination.add_image(image, None);
  if !destination.finalize() {
    return Err("Core Graphics could not encode a preview image".to_owned());
  }
  Ok(data.as_slice().to_vec())
}

fn images_at(
  screen: &av::AssetImageGenerator,
  camera: Option<&av::AssetImageGenerator>,
  position_ms: u64,
) -> Result<(Vec<u8>, Option<Vec<u8>>), String> {
  let time = cm::Time::new(position_ms as i64, 1_000);
  if let Some(camera) = camera {
    let (screen, camera) = tauri::async_runtime::block_on(async {
      tokio::join!(
        screen.cg_image_for_time(time),
        camera.cg_image_for_time(time)
      )
    });
    let (screen, _) = screen.map_err(|error| error.to_string())?;
    let (camera, _) = camera.map_err(|error| error.to_string())?;
    Ok((jpeg(&screen)?, Some(jpeg(&camera)?)))
  } else {
    let (screen, _) = tauri::async_runtime::block_on(screen.cg_image_for_time(time))
      .map_err(|error| error.to_string())?;
    Ok((jpeg(&screen)?, None))
  }
}

pub(super) fn send_frame(
  channel: &Channel,
  request_id: u64,
  screen: &[u8],
  camera: Option<&[u8]>,
) -> bool {
  let camera = camera.unwrap_or_default();
  let Ok(screen_len) = u32::try_from(screen.len()) else {
    return false;
  };
  let Ok(camera_len) = u32::try_from(camera.len()) else {
    return false;
  };
  let mut payload = Vec::with_capacity(24 + screen.len() + camera.len());
  payload.extend_from_slice(&NATIVE_FRAME_MARKER.to_le_bytes());
  payload.extend_from_slice(&1_u32.to_le_bytes());
  payload.extend_from_slice(&request_id.to_le_bytes());
  payload.extend_from_slice(&screen_len.to_le_bytes());
  payload.extend_from_slice(&camera_len.to_le_bytes());
  payload.extend_from_slice(screen);
  payload.extend_from_slice(camera);
  channel.send(InvokeResponseBody::Raw(payload)).is_ok()
}

fn run(
  sources: PlayerSources,
  receiver: mpsc::Receiver<DecoderCommand>,
  frame_channel: Channel,
  event_channel: Channel<RecordingPreviewPlayerEvent>,
) {
  let mut screen = match image_generator(&sources.screen_path) {
    Ok(generator) => generator,
    Err(message) => {
      let _ = event_channel.send(RecordingPreviewPlayerEvent::Error { message });
      return;
    }
  };
  let mut camera = match sources.camera_path.as_deref().map(image_generator) {
    Some(Ok(generator)) => Some(generator),
    Some(Err(message)) => {
      let _ = event_channel.send(RecordingPreviewPlayerEvent::Error { message });
      return;
    }
    None => None,
  };

  while let Ok(command) = receiver.recv() {
    let DecoderCommand::Seek {
      full_resolution,
      position_ms,
      request_id,
    } = command
    else {
      break;
    };
    set_size(
      &mut screen,
      &sources.playback_layout.panes[0],
      full_resolution,
    );
    if let (Some(generator), Some(pane)) = (camera.as_mut(), sources.playback_layout.panes.get(1)) {
      set_size(generator, pane, full_resolution);
    }
    match images_at(&screen, camera.as_deref(), position_ms) {
      Ok((screen, camera)) => {
        if send_frame(&frame_channel, request_id, &screen, camera.as_deref()) {
          let _ = event_channel.send(RecordingPreviewPlayerEvent::Ready {
            position_ms,
            request_id,
          });
        }
      }
      Err(message) => {
        let _ = event_channel.send(RecordingPreviewPlayerEvent::Error { message });
      }
    }
  }
}

impl NativeStillDecoder {
  pub(super) fn spawn(
    sources: PlayerSources,
    frame_channel: Channel,
    event_channel: Channel<RecordingPreviewPlayerEvent>,
  ) -> Result<Self, String> {
    let (sender, receiver) = mpsc::channel();
    let thread = std::thread::Builder::new()
      .name("recording-preview-still".to_owned())
      .spawn(move || run(sources, receiver, frame_channel, event_channel))
      .map_err(|error| error.to_string())?;
    Ok(Self {
      sender,
      thread: Some(thread),
    })
  }

  pub(super) fn seek(
    &self,
    position_ms: u64,
    request_id: u64,
    full_resolution: bool,
  ) -> Result<(), String> {
    self
      .sender
      .send(DecoderCommand::Seek {
        full_resolution,
        position_ms,
        request_id,
      })
      .map_err(|_| "The native preview decoder stopped".to_owned())
  }

  pub(super) fn stop(mut self) {
    let _ = self.sender.send(DecoderCommand::Stop);
    if let Some(thread) = self.thread.take() {
      let _ = thread.join();
    }
  }
}
