// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::super::*;
use super::{screen_stream, video_source::PrimaryVideo};
use crate::recording::SystemAudioSelection;

#[derive(Default)]
pub(super) struct SystemAudioStreams {
  pub all: Option<arc::R<sc::Stream>>,
  pub selected: Option<arc::R<sc::Stream>>,
  pub video_captures_all: bool,
}

pub(super) fn create(
  selection: &SystemAudioSelection,
  content: Option<&sc::ShareableContent>,
  output: Option<&arc::R<ScreenOutput>>,
  queue: &dispatch::Queue,
  video: Option<&PrimaryVideo>,
) -> Result<SystemAudioStreams, String> {
  let captures_selected = selection.enabled && !selection.application_ids.is_empty();
  let captures_all = selection.enabled && !captures_selected;
  let video_captures_all =
    captures_all && video.is_some_and(|primary_video| !primary_video.is_window);
  if !selection.enabled {
    return Ok(SystemAudioStreams {
      all: None,
      selected: None,
      video_captures_all,
    });
  }

  let content = content.expect("audio has content");
  let displays = content.displays();
  let display = displays
    .first()
    .ok_or_else(|| "No monitor is available for audio capture".to_owned())?;
  let output = output.expect("content has output");
  let all = if captures_all && !video_captures_all {
    Some(screen_stream::create_all_audio(
      screen_stream::AllAudioStreamRequest {
        content,
        display,
        output,
        queue,
      },
    )?)
  } else {
    None
  };
  let selected = if captures_selected {
    let filter = application_audio_filter(content, display, &selection.application_ids)?;
    let mut cfg = sc::StreamCfg::new();
    cfg.set_captures_audio(true);
    screen_stream::configure_audio(&mut cfg);
    let stream = sc::Stream::new(&filter, &cfg);
    stream
      .add_stream_output(output.as_ref(), sc::OutputType::Audio, Some(queue))
      .map_err(|error| error.to_string())?;
    Some(stream)
  } else {
    None
  };

  Ok(SystemAudioStreams {
    all,
    selected,
    video_captures_all,
  })
}
