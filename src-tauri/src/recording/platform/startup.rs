// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use super::{camera::CameraSpec, writer::VideoSource};

mod audio_stream;
mod camera_writer;
mod screen_stream;
mod video_source;
mod writer_thread;

use audio_stream::SystemAudioStreams;
use camera_writer::CameraWriterSetup;
use screen_stream::VideoStreamRequest;
use writer_thread::{both_first_frames, spawn_writer, WriterThread};

pub(super) async fn begin(config: CaptureStartupConfig) -> Result<CaptureStart, String> {
  let CaptureStartupConfig {
    camera,
    camera_path,
    microphone_id,
    on_failure,
    path,
    primary,
    system_audio,
  } = config;
  let camera_primary = matches!(primary, PrimaryCaptureSource::Camera);
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
  let primary_video = content
    .as_deref()
    .map(|content| video_source::resolve(content, &primary))
    .transpose()?
    .flatten();
  let source_scale_factor = primary_video
    .as_ref()
    .map_or(1.0, |video| video.source_scale_factor);
  let (width, height, primary_fps) = if camera_primary {
    let camera = camera_spec.as_ref().expect("checked above");
    (camera.width, camera.height, camera.fps)
  } else {
    let video = primary_video
      .as_ref()
      .ok_or_else(|| "No video source is available for recording".to_owned())?;
    (video.width, video.height, video.fps)
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

  let CameraWriterSetup {
    first_frame: camera_first_frame,
    primary_spec: primary_camera_spec,
    secondary: secondary_camera,
  } = camera_writer::prepare(
    camera_spec,
    camera_primary,
    camera_flipped,
    camera_path,
    &timeline_origin,
    &on_failure,
  )?;
  let mut primary_camera = None;

  let output = content.as_ref().map(|_| {
    ScreenOutput::with(ScreenOutputInner {
      commands: commands.clone(),
      stats: Arc::clone(&stats),
    })
  });
  let queue = dispatch::Queue::serial_with_ar_pool();
  let mut streams = Vec::new();
  let SystemAudioStreams {
    all: all_audio_stream,
    selected: selected_audio_stream,
    video_captures_all: video_captures_all_audio,
  } = audio_stream::create(
    &system_audio,
    content.as_deref(),
    output.as_ref(),
    &queue,
    primary_video.as_ref(),
  )?;
  let video_stream = primary_video
    .as_ref()
    .map(|video| {
      screen_stream::create_video(VideoStreamRequest {
        captures_audio: video_captures_all_audio,
        output: output.as_ref().expect("content has output"),
        queue: &queue,
        video,
      })
    })
    .transpose()?;

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
  if let Some(stream) = &all_audio_stream {
    if let Err(error) = stream.start().await {
      if let Some(stream) = &selected_audio_stream {
        stream.stop_with_ch(|_| {});
      }
      return Err(error.to_string());
    }
  }
  if let Some(stream) = &video_stream {
    if let Err(error) = stream.start().await {
      if let Some(stream) = &selected_audio_stream {
        stream.stop_with_ch(|_| {});
      }
      if let Some(stream) = &all_audio_stream {
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
  if let Some(stream) = video_stream {
    streams.push(stream);
  }
  if let Some(stream) = all_audio_stream {
    streams.push(stream);
  }
  if let Some(stream) = selected_audio_stream {
    streams.push(stream);
  }

  let first_frame = match camera_first_frame {
    Some(camera) => both_first_frames(first_frame, camera),
    None => first_frame,
  };
  Ok(CaptureStart {
    session: CaptureSession {
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
    source_scale_factor,
  })
}
