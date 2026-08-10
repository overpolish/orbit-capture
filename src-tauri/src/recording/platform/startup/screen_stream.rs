// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::super::*;
use crate::capture_geometry::CaptureRect;

pub(super) struct ScreenStreamRequest<'a> {
  pub camera_primary: bool,
  pub captures_selected_audio: bool,
  pub content: Option<&'a sc::ShareableContent>,
  pub display: Option<&'a sc::Display>,
  pub display_scale: f64,
  pub fps: u32,
  pub height: u32,
  pub output: Option<&'a arc::R<ScreenOutput>>,
  pub queue: &'a dispatch::Queue,
  pub show_cursor: bool,
  pub source_rect: Option<CaptureRect>,
  pub system_audio: bool,
  pub width: u32,
}

pub(super) fn create(
  request: ScreenStreamRequest<'_>,
) -> Result<Option<arc::R<sc::Stream>>, String> {
  let ScreenStreamRequest {
    camera_primary,
    captures_selected_audio,
    content,
    display,
    display_scale,
    fps,
    height,
    output,
    queue,
    show_cursor,
    source_rect,
    system_audio,
    width,
  } = request;
  if camera_primary && (!system_audio || captures_selected_audio) {
    return Ok(None);
  }
  let content = content.expect("required for display capture");
  let display = display.expect("required for display capture");

  let mut cfg = sc::StreamCfg::new();
  cfg.set_width(width as usize);
  cfg.set_height(height as usize);
  if let Some(rect) = source_rect {
    // ScreenCaptureKit's source rectangle is expressed in display points,
    // while the shared source contract is physical pixels.
    cfg.set_src_rect(cg::Rect::new(
      f64::from(rect.x) / display_scale,
      f64::from(rect.y) / display_scale,
      f64::from(rect.width) / display_scale,
      f64::from(rect.height) / display_scale,
    ));
  }
  cfg.set_pixel_format(cv::PixelFormat::_420V);
  cfg.set_minimum_frame_interval(cm::Time::new(1, fps as cm::TimeScale));
  cfg.set_queue_depth(STREAM_QUEUE_DEPTH);
  cfg.set_shows_cursor(show_cursor && !camera_primary);
  cfg.set_captures_audio(system_audio && !captures_selected_audio);
  if system_audio {
    cfg.set_excludes_current_process_audio(true);
    cfg.set_sample_rate(SYSTEM_AUDIO_SAMPLE_RATE);
    cfg.set_channel_count(SYSTEM_AUDIO_CHANNELS);
  }
  cfg.set_color_space_name(cg::color_space::names::srgb());

  let filter = sc::ContentFilter::with_display_excluding_windows(display, &our_windows(content));
  let stream = sc::Stream::new(&filter, &cfg);
  if !camera_primary {
    stream
      .add_stream_output(
        output.expect("content has output").as_ref(),
        sc::OutputType::Screen,
        Some(queue),
      )
      .map_err(|error| error.to_string())?;
  }
  if system_audio && !captures_selected_audio {
    stream
      .add_stream_output(
        output.expect("content has output").as_ref(),
        sc::OutputType::Audio,
        Some(queue),
      )
      .map_err(|error| error.to_string())?;
  }

  Ok(Some(stream))
}
