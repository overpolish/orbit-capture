// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use super::{camera::CameraSpec, session::CameraObjects, writer::VideoSource};

mod writer_thread;

use writer_thread::{both_first_frames, spawn_writer, WriterThread};

pub(super) async fn begin(
  config: CaptureStartupConfig,
) -> Result<(CaptureSession, Receiver<Result<(), String>>), String> {
  let CaptureStartupConfig {
    camera,
    camera_path,
    microphone_id,
    on_failure,
    path,
    primary,
    system_audio,
  } = config;
  let (monitor_id, show_cursor, fps, camera_primary) = match primary {
    PrimaryCaptureSource::Screen {
      fps,
      monitor_id,
      show_cursor,
    } => (Some(monitor_id), show_cursor, fps, false),
    PrimaryCaptureSource::Camera { fps } => (None, false, fps, true),
  };
  let camera_flipped = camera.as_ref().is_some_and(|camera| camera.flipped);
  let camera_spec = camera.map(CameraSpec::resolve).transpose()?;
  if camera_primary && camera_spec.is_none() {
    return Err("No camera is selected to record".to_owned());
  }

  let needs_content = !camera_primary || system_audio.enabled;
  let content = if needs_content {
    Some(
      sc::ShareableContent::current()
        .await
        .map_err(|error| error.to_string())?,
    )
  } else {
    None
  };
  let displays = content.as_ref().map(|content| content.displays());
  let display = displays.as_ref().and_then(|displays| {
    monitor_id
      .and_then(|id| displays.iter().find(|display| display.display_id().0 == id))
      .or_else(|| displays.first())
  });
  if needs_content && display.is_none() {
    return Err("No monitor is available for recording".to_owned());
  }

  let (width, height, primary_fps) = if camera_primary {
    let camera = camera_spec.as_ref().expect("checked above");
    (camera.width, camera.height, camera.fps)
  } else {
    let monitor_id = monitor_id.ok_or_else(|| "No monitor is selected to record".to_owned())?;
    let (_, width, height) = monitor_geometry(monitor_id)?;
    (even(width), even(height), fps)
  };
  if width == 0 || height == 0 {
    return Err("The selected video source has no usable size".to_owned());
  }

  let microphone_source = microphone_id
    .as_deref()
    .map(MicrophoneSource::resolve)
    .transpose()?;
  let microphone_format = microphone_source.as_ref().map(MicrophoneSource::format);
  let timeline_origin = Arc::new(OnceLock::new());
  let stats = Arc::new(CaptureStats::default());
  // Orbit Cursor's proven macOS workaround used two independent HEVC
  // VideoToolbox encoders. A concurrent H.264 + HEVC pair competes for the
  // hardware path and made the camera writer fall behind by roughly 20%.
  let primary_encoder = if camera_spec.is_some() && !camera_primary {
    VideoEncoder::Hevc
  } else {
    VideoEncoder::H264
  };
  let WriterThread {
    commands,
    first_frame,
    worker,
  } = spawn_writer(
    WriterConfig {
      path,
      width,
      height,
      fps: primary_fps,
      encoder: primary_encoder,
      system_audio: system_audio.enabled,
      microphone_format,
      stats: Arc::clone(&stats),
      on_failure: Arc::clone(&on_failure),
      container: Container::quicktime_fragmented(),
      primary_video: true,
      source: if camera_primary {
        VideoSource::Camera
      } else {
        VideoSource::Screen
      },
      timeline_origin: Arc::clone(&timeline_origin),
    },
    "orbit-recording-writer",
  )?;

  let mut secondary_camera = None;
  let mut primary_camera = None;
  let mut primary_camera_spec = None;
  let mut camera_first_frame = None;
  if let Some(spec) = camera_spec {
    if camera_primary {
      primary_camera_spec = Some(spec);
    } else {
      let camera_path = camera_path.ok_or_else(|| "The camera has nowhere to record".to_owned())?;
      let camera_stats = Arc::new(CaptureStats::default());
      let WriterThread {
        commands: camera_commands,
        first_frame: first_camera,
        worker: camera_worker,
      } = spawn_writer(
        WriterConfig {
          path: camera_path.clone(),
          width: spec.width,
          height: spec.height,
          fps: spec.fps,
          // Both concurrent video writers use HEVC so VideoToolbox can keep
          // independent hardware-backed sessions, matching Orbit Cursor's
          // multi-video capture path on macOS.
          encoder: VideoEncoder::Hevc,
          system_audio: false,
          microphone_format: None,
          stats: Arc::clone(&camera_stats),
          on_failure: Arc::clone(&on_failure),
          container: Container::quicktime_fragmented(),
          primary_video: false,
          source: VideoSource::Camera,
          timeline_origin: Arc::clone(&timeline_origin),
        },
        "orbit-camera-writer",
      )?;
      let stream = camera::start(spec, camera_flipped, camera_commands.clone(), camera_stats)?;
      camera_first_frame = Some(first_camera);
      secondary_camera = Some(CameraObjects {
        commands: camera_commands,
        path: camera_path,
        stream: Some(stream),
        worker: Some(camera_worker),
      });
    }
  }

  let captures_selected_audio = system_audio.enabled && !system_audio.application_ids.is_empty();
  let output = content.as_ref().map(|_| {
    ScreenOutput::with(ScreenOutputInner {
      commands: commands.clone(),
      stats: Arc::clone(&stats),
    })
  });
  let queue = dispatch::Queue::serial_with_ar_pool();
  let mut streams = Vec::new();

  let screen_stream = if !camera_primary || system_audio.enabled && !captures_selected_audio {
    let content = content.as_ref().expect("required above");
    let display = display.expect("required above");
    let mut cfg = sc::StreamCfg::new();
    cfg.set_width(width as usize);
    cfg.set_height(height as usize);
    cfg.set_pixel_format(cv::PixelFormat::_420V);
    cfg.set_minimum_frame_interval(cm::Time::new(1, fps as cm::TimeScale));
    cfg.set_queue_depth(STREAM_QUEUE_DEPTH);
    cfg.set_shows_cursor(show_cursor && !camera_primary);
    cfg.set_captures_audio(system_audio.enabled && !captures_selected_audio);
    if system_audio.enabled {
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
          output.as_ref().expect("content has output").as_ref(),
          sc::OutputType::Screen,
          Some(&queue),
        )
        .map_err(|error| error.to_string())?;
    }
    if system_audio.enabled && !captures_selected_audio {
      stream
        .add_stream_output(
          output.as_ref().expect("content has output").as_ref(),
          sc::OutputType::Audio,
          Some(&queue),
        )
        .map_err(|error| error.to_string())?;
    }
    Some(stream)
  } else {
    None
  };

  let selected_audio_stream = if captures_selected_audio {
    let content = content.as_ref().expect("required above");
    let display = display.expect("required above");
    let filter = application_audio_filter(content, display, &system_audio.application_ids)?;
    let mut cfg = sc::StreamCfg::new();
    cfg.set_captures_audio(true);
    cfg.set_excludes_current_process_audio(true);
    cfg.set_sample_rate(SYSTEM_AUDIO_SAMPLE_RATE);
    cfg.set_channel_count(SYSTEM_AUDIO_CHANNELS);
    let stream = sc::Stream::new(&filter, &cfg);
    stream
      .add_stream_output(
        output.as_ref().expect("content has output").as_ref(),
        sc::OutputType::Audio,
        Some(&queue),
      )
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

  if let Some(stream) = &selected_audio_stream {
    stream.start().await.map_err(|error| error.to_string())?;
  }
  if let Some(stream) = &screen_stream {
    if let Err(error) = stream.start().await {
      if let Some(stream) = &selected_audio_stream {
        stream.stop_with_ch(|_| {});
      }
      return Err(error.to_string());
    }
  }
  if let Some(spec) = primary_camera_spec {
    // Audio is live first and waits in the writer's bounded preroll. The
    // camera's first stable frame then establishes the shared time zero, so
    // neither microphone nor system audio begins late.
    primary_camera = Some(camera::start(
      spec,
      camera_flipped,
      commands.clone(),
      Arc::clone(&stats),
    )?);
  }
  if let Some(stream) = screen_stream {
    streams.push(stream);
  }
  if let Some(stream) = selected_audio_stream {
    streams.push(stream);
  }

  let first_frame = match camera_first_frame {
    Some(camera) => both_first_frames(first_frame, camera),
    None => first_frame,
  };
  Ok((
    CaptureSession {
      camera: secondary_camera,
      commands,
      microphone,
      objects: StreamObjects {
        _output: output,
        queue,
        streams,
      },
      primary_camera,
      worker: Some(worker),
    },
    first_frame,
  ))
}
