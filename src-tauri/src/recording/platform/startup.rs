// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// The writer thread, once it has confirmed it can write.
struct WriterThread {
  commands: SyncSender<Command>,
  first_frame: Receiver<Result<(), String>>,
  worker: JoinHandle<()>,
}

/// Starts the writer thread and waits for it to report that it is ready.
fn spawn_writer(config: WriterConfig) -> Result<WriterThread, String> {
  let (commands, inbox) = mpsc::sync_channel(FRAME_QUEUE_DEPTH);
  let (ready, readied) = mpsc::channel();
  let (first_frame, first_framed) = mpsc::channel();

  let worker = std::thread::Builder::new()
    .name("orbit-recording-writer".to_owned())
    .spawn(move || match Writer::new(config) {
      Ok(writer) => {
        let _ = ready.send(Ok(()));
        writer.run(&inbox, &first_frame);
      }
      Err(error) => {
        let _ = ready.send(Err(error));
      }
    })
    .map_err(|error| error.to_string())?;

  match readied.recv() {
    Ok(Ok(())) => Ok(WriterThread {
      commands,
      first_frame: first_framed,
      worker,
    }),
    Ok(Err(error)) => {
      let _ = worker.join();
      Err(error)
    }
    Err(_) => {
      let _ = worker.join();
      Err("The recording's encoder could not be started".to_owned())
    }
  }
}

async fn begin(
  monitor_id: u32,
  show_cursor: bool,
  system_audio: SystemAudioSelection,
  microphone_id: Option<String>,
  fps: u32,
  path: PathBuf,
  on_failure: FailureReport,
) -> Result<(CaptureSession, Receiver<Result<(), String>>), String> {
  let content = sc::ShareableContent::current()
    .await
    .map_err(|error| error.to_string())?;
  let displays = content.displays();
  let display = displays
    .iter()
    .find(|display| display.display_id().0 == monitor_id)
    .ok_or_else(|| "The selected monitor is no longer available".to_owned())?;
  let (_, width, height) = monitor_geometry(monitor_id)?;
  let (width, height) = (even(width), even(height));
  if width == 0 || height == 0 {
    return Err("The selected monitor has no usable size".to_owned());
  }

  let microphone_source = microphone_id
    .as_deref()
    .map(MicrophoneSource::resolve)
    .transpose()?;
  let microphone_format = microphone_source.as_ref().map(MicrophoneSource::format);

  let stats = Arc::new(CaptureStats::default());
  let captures_selected_audio = system_audio.enabled && !system_audio.application_ids.is_empty();
  let WriterThread {
    commands,
    first_frame,
    worker,
  } = spawn_writer(WriterConfig {
    path,
    width,
    height,
    fps,
    system_audio: system_audio.enabled,
    microphone_format,
    stats: Arc::clone(&stats),
    on_failure,
    container: Container::quicktime_fragmented(),
  })?;

  let mut cfg = sc::StreamCfg::new();
  cfg.set_width(width as usize);
  cfg.set_height(height as usize);
  // NV12 is what the encoder wants, so asking for it here is what keeps the
  // frame from being converted twice on its way to the file.
  cfg.set_pixel_format(cv::PixelFormat::_420V);
  cfg.set_minimum_frame_interval(cm::Time::new(1, fps as cm::TimeScale));
  cfg.set_queue_depth(STREAM_QUEUE_DEPTH);
  cfg.set_shows_cursor(show_cursor);
  cfg.set_captures_audio(system_audio.enabled && !captures_selected_audio);
  if system_audio.enabled {
    cfg.set_excludes_current_process_audio(true);
    cfg.set_sample_rate(SYSTEM_AUDIO_SAMPLE_RATE);
    cfg.set_channel_count(SYSTEM_AUDIO_CHANNELS);
  }
  // Standard dynamic range, which is what an H.264 file can carry. The
  // dedicated SDR switch is macOS 15, so the colour space says it instead.
  cfg.set_color_space_name(cg::color_space::names::srgb());

  let filter = sc::ContentFilter::with_display_excluding_windows(display, &our_windows(&content));
  let output = ScreenOutput::with(ScreenOutputInner {
    commands: commands.clone(),
    stats: Arc::clone(&stats),
  });
  // Without the autorelease pool the IOSurface-backed frames pile up until the
  // run loop gets round to draining them, which for a capture is never.
  let queue = dispatch::Queue::serial_with_ar_pool();
  let screen_stream = sc::Stream::new(&filter, &cfg);
  screen_stream
    .add_stream_output(output.as_ref(), sc::OutputType::Screen, Some(&queue))
    .map_err(|error| error.to_string())?;
  if system_audio.enabled && !captures_selected_audio {
    screen_stream
      .add_stream_output(output.as_ref(), sc::OutputType::Audio, Some(&queue))
      .map_err(|error| error.to_string())?;
  }

  let selected_audio_stream = if captures_selected_audio {
    let audio_filter = application_audio_filter(&content, display, &system_audio.application_ids)?;
    let mut audio_cfg = sc::StreamCfg::new();
    audio_cfg.set_captures_audio(true);
    audio_cfg.set_excludes_current_process_audio(true);
    audio_cfg.set_sample_rate(SYSTEM_AUDIO_SAMPLE_RATE);
    audio_cfg.set_channel_count(SYSTEM_AUDIO_CHANNELS);
    let stream = sc::Stream::new(&audio_filter, &audio_cfg);
    stream
      .add_stream_output(output.as_ref(), sc::OutputType::Audio, Some(&queue))
      .map_err(|error| error.to_string())?;
    Some(stream)
  } else {
    None
  };

  let microphone = if let Some(source) = microphone_source {
    let sample_commands = commands.clone();
    let sample_stats = Arc::clone(&stats);
    let on_buffer = Arc::new(move |buffer| {
      if let Err(TrySendError::Full(_)) = sample_commands.try_send(Command::Microphone(buffer)) {
        sample_stats
          .microphone_dropped
          .fetch_add(1, Ordering::Relaxed);
      }
    });
    let error_commands = commands.clone();
    let on_error = Arc::new(move |error| {
      let _ = error_commands.send(Command::MicrophoneFailed(error));
    });
    Some(source.start(on_buffer, on_error)?)
  } else {
    None
  };

  // Microphone capture is already running here. Start filtered system audio
  // next so both initial buffers are waiting when video establishes time zero;
  // the writer trims each pre-roll at sample accuracy.
  if let Some(stream) = &selected_audio_stream {
    stream.start().await.map_err(|error| error.to_string())?;
  }
  if let Err(error) = screen_stream.start().await {
    if let Some(stream) = &selected_audio_stream {
      stream.stop_with_ch(|_| {});
    }
    return Err(error.to_string());
  }

  let mut streams = Vec::with_capacity(1 + usize::from(selected_audio_stream.is_some()));
  streams.push(screen_stream);
  if let Some(stream) = selected_audio_stream {
    streams.push(stream);
  }
  let session = CaptureSession {
    commands,
    microphone,
    objects: StreamObjects {
      _output: output,
      queue,
      streams,
    },
    worker: Some(worker),
  };

  Ok((session, first_frame))
}

/// ScreenCaptureKit is an Objective-C conversation, so the whole setup is
/// confined to one blocking thread the way still capture is.
pub fn begin_blocking(
  monitor_id: u32,
  show_cursor: bool,
  system_audio: SystemAudioSelection,
  microphone_id: Option<String>,
  fps: u32,
  path: PathBuf,
  on_failure: FailureReport,
) -> Result<(CaptureSession, Receiver<Result<(), String>>), String> {
  tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .map_err(|error| error.to_string())?
    .block_on(begin(
      monitor_id,
      show_cursor,
      system_audio,
      microphone_id,
      fps,
      path,
      on_failure,
    ))
}
