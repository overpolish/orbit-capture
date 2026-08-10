// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::super::audio_writer::AudioWriter;
use super::super::*;
use super::audio_stream;
use super::microphone_stream;
use crate::recording::encoding::FailureReport;
use crate::recording::SystemAudioSelection;

pub(super) async fn begin(
  microphone_id: Option<String>,
  on_failure: FailureReport,
  path: PathBuf,
  system_audio: SystemAudioSelection,
) -> Result<CaptureStart, String> {
  let microphone_source = microphone_id
    .as_deref()
    .map(MicrophoneSource::resolve)
    .transpose()?;
  let microphone_format = microphone_source.as_ref().map(MicrophoneSource::format);
  let stats = Arc::new(CaptureStats::default());
  let (commands, inbox) = mpsc::sync_channel(FRAME_QUEUE_DEPTH);
  let (first_frame, first_framed) = mpsc::channel();
  let writer = AudioWriter::new(
    path,
    system_audio.enabled,
    microphone_format,
    Arc::clone(&stats),
    Arc::clone(&on_failure),
  )?;
  let worker = std::thread::Builder::new()
    .name("orbit-audio-writer".to_owned())
    .spawn(move || writer.run(&inbox, first_frame))
    .map_err(|error| error.to_string())?;

  let content = if system_audio.enabled {
    Some(
      sc::ShareableContent::current()
        .await
        .map_err(|error| error.to_string())?,
    )
  } else {
    None
  };
  let output = content.as_ref().map(|_| {
    ScreenOutput::with(ScreenOutputInner {
      commands: commands.clone(),
      stats: Arc::clone(&stats),
    })
  });
  let queue = dispatch::Queue::serial_with_ar_pool();
  let system_audio_streams = audio_stream::create(
    &system_audio,
    content.as_deref(),
    output.as_ref(),
    &queue,
    None,
  )?;
  let microphone = microphone_stream::start(microphone_source, &commands, &stats)?;
  system_audio_streams.start().await?;
  commands
    .send(Command::Begin { at: Instant::now() })
    .map_err(|_| "The audio recording writer stopped during startup".to_owned())?;

  let mut streams = Vec::new();
  system_audio_streams.append_to(&mut streams);
  Ok(CaptureStart {
    first_frame: first_framed,
    session: CaptureSession {
      camera: None,
      commands,
      microphone,
      objects: StreamObjects {
        _output: output,
        queue,
        streams,
      },
      primary_camera: None,
      worker: Some(worker),
    },
    source_scale_factor: 1.0,
  })
}
