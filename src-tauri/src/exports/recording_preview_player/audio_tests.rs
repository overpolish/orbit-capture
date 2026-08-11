// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::exports::recording_preview_player::layout::PreviewPaneKind;
use crate::exports::{AudioTrackKind, RecordingAudioTrack};

#[test]
fn decodes_tracks_into_independent_pcm_channels() {
  let layout = super::super::preview_layout(
    Some((1_920, 1_080, PreviewPaneKind::Screen)),
    None,
    super::super::PREVIEW_HEIGHT,
  );
  let sources = PlayerSources {
    audio_tracks: vec![
      RecordingAudioTrack {
        kind: AudioTrackKind::SystemAudio,
        label: "System audio".to_owned(),
        stream_index: 0,
      },
      RecordingAudioTrack {
        kind: AudioTrackKind::Microphone,
        label: "Microphone".to_owned(),
        stream_index: 1,
      },
    ],
    camera_duration_ms: None,
    camera_path: None,
    duration_ms: 1_000,
    layout: layout.clone(),
    playback_layout: layout,
    screen_path: "/tmp/recording.mov".into(),
  };
  let config = StreamConfig {
    channels: 2,
    sample_rate: 48_000,
    buffer_size: cpal::BufferSize::Default,
  };
  let rendered = args(&sources, 250, &config).join(" ");

  assert!(rendered.contains("[0:a:0]aresample=48000"));
  assert!(rendered.contains("[0:a:1]aresample=48000"));
  assert!(rendered.contains("apad=whole_dur=0.750"));
  assert!(rendered.contains("amerge=inputs=2[tracks]"));
  assert!(rendered.contains("-ac 2"));
}
