// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

mod container;
mod finish;
mod samples;

use std::{
  collections::VecDeque,
  io::Cursor,
  path::PathBuf,
  sync::{
    atomic::Ordering,
    mpsc::{self, Receiver},
    Arc,
  },
  time::Instant,
};

use cidre::{arc, av, cm, ns};

use super::{
  media::{
    audio_sample_from_origin, audio_sample_with_pts, microphone_audio_settings,
    microphone_buffer_from_origin, microphone_format_description, microphone_sample_buffer, nanos,
    system_audio_settings, time_to_ns, video_settings,
  },
  AudioSample, CaptureStats, Command, Frame, MICROPHONE_PREROLL_LIMIT, NANOS_PER_MS,
  POSTER_MAX_EDGE, REJECTION_STREAK_LIMIT, SYSTEM_AUDIO_PREROLL_LIMIT, TAIL_APPEND_ATTEMPTS,
  TAIL_APPEND_WAIT,
};
pub(super) use container::Container;
use finish::writer_error;

use crate::recording::{
  encoding::{nv12_poster_rgba, poster_size, FailureReport, FinalizeInfo, Plane, Timeline},
  microphone::{Buffer as MicrophoneBuffer, Format as MicrophoneFormat},
};

/// The writer thread's whole world. Created on that thread, dropped on it.
pub(super) struct Writer {
  adaptor: arc::R<av::asset::WriterInputPixelBufAdaptor>,
  pub(in crate::recording::platform) base: Instant,
  pub(super) height: u32,
  input: arc::R<av::AssetWriterInput>,
  system_audio_input: Option<arc::R<av::AssetWriterInput>>,
  last_system_audio_pts_ns: Option<i64>,
  microphone_input: Option<arc::R<av::AssetWriterInput>>,
  pub(super) microphone_format: Option<MicrophoneFormat>,
  microphone_format_description: Option<arc::R<cm::AudioFormatDesc>>,
  last_microphone_pts_ns: Option<i64>,
  microphone_end_ns: Option<i64>,
  microphone_failure_reported: bool,
  origin_source_ns: Option<i64>,
  origin_wall: Option<Instant>,
  system_audio_end_ns: Option<i64>,
  /// Set once the writer has refused to carry on. Everything downstream
  /// checks this rather than asking AVFoundation again per frame.
  failed: Option<String>,
  /// The timestamp of the last frame that actually reached the file. The movie
  /// ends here, because this is where its media ends.
  last_appended_ns: Option<i64>,
  pub(super) on_failure: FailureReport,
  pub(super) path: PathBuf,
  pending_microphone: VecDeque<MicrophoneBuffer>,
  pending_system_audio: VecDeque<AudioSample>,
  rejection_streak: u64,
  pub(super) stats: Arc<CaptureStats>,
  /// The last frame seen, appended once more at the true stop time. Without
  /// it a recording of a screen that stopped changing ends at its last change
  /// rather than when the user stopped it.
  tail: Option<Frame>,
  timeline: Timeline,
  pub(super) width: u32,
  writer: arc::R<av::AssetWriter>,
}

pub(super) struct WriterConfig {
  pub(super) path: PathBuf,
  pub(super) width: u32,
  pub(super) height: u32,
  pub(super) fps: u32,
  pub(super) system_audio: bool,
  pub(super) microphone_format: Option<MicrophoneFormat>,
  pub(super) stats: Arc<CaptureStats>,
  pub(super) on_failure: FailureReport,
  /// Always `Container::quicktime_fragmented()` in the app. It is a field
  /// rather than a constant so the encoder tests can drive a real writer at
  /// the containers that were rejected and keep proving why.
  pub(super) container: Container,
}

impl Writer {
  pub(super) fn new(config: WriterConfig) -> Result<Self, String> {
    let WriterConfig {
      path,
      width,
      height,
      fps,
      system_audio,
      microphone_format,
      stats,
      on_failure,
      container,
    } = config;
    let location = path
      .to_str()
      .ok_or_else(|| "The recording's location cannot be written as text".to_owned())?;
    let url = ns::Url::with_fs_path_str(location, false);
    let mut writer = av::AssetWriter::with_url_and_file_type(&url, container.format.file_type())
      .map_err(|error| error.to_string())?;
    // Set before any input is added, and only for a container that can take
    // it: fragmenting an .mp4 through this pipeline fails the writer outright.
    // `Container::quicktime_fragmented` carries the whole argument.
    if let Some(interval) = container.fragment_interval {
      writer.set_movie_fragment_interval(interval);
    }

    let settings = video_settings(width, height, fps);
    let mut input = av::AssetWriterInput::with_media_type_and_output_settings(
      av::MediaType::video(),
      Some(&settings),
    )
    .map_err(|error| error.to_string())?;
    // Frames arrive as fast as the screen changes and no faster, so the input
    // must not wait for a backlog it will never get.
    input.set_expects_media_data_in_real_time(true);

    let adaptor = av::asset::WriterInputPixelBufAdaptor::with_input_writer(&input, None)
      .map_err(|error| error.to_string())?;
    writer
      .add_input(&input)
      .map_err(|error| error.to_string())?;

    let system_audio_input = if system_audio {
      let settings = system_audio_settings();
      let mut audio_input = av::AssetWriterInput::with_media_type_and_output_settings(
        av::MediaType::audio(),
        Some(&settings),
      )
      .map_err(|error| error.to_string())?;
      audio_input.set_expects_media_data_in_real_time(true);
      writer
        .add_input(&audio_input)
        .map_err(|error| error.to_string())?;
      Some(audio_input)
    } else {
      None
    };

    let (microphone_input, microphone_format_description) = if let Some(format) = microphone_format
    {
      let settings = microphone_audio_settings(format);
      let mut audio_input = av::AssetWriterInput::with_media_type_and_output_settings(
        av::MediaType::audio(),
        Some(&settings),
      )
      .map_err(|error| error.to_string())?;
      audio_input.set_expects_media_data_in_real_time(true);
      writer
        .add_input(&audio_input)
        .map_err(|error| error.to_string())?;
      (
        Some(audio_input),
        Some(microphone_format_description(format)?),
      )
    } else {
      (None, None)
    };

    if !writer.start_writing() {
      return Err(writer_error(&writer, "The recording could not be started"));
    }
    writer.start_session_at_src_time(cm::Time::zero());

    Ok(Self {
      adaptor,
      base: Instant::now(),
      failed: None,
      last_appended_ns: None,
      height,
      input,
      last_microphone_pts_ns: None,
      last_system_audio_pts_ns: None,
      microphone_end_ns: None,
      microphone_failure_reported: false,
      microphone_format,
      microphone_format_description,
      microphone_input,
      on_failure,
      origin_source_ns: None,
      origin_wall: None,
      path,
      pending_microphone: VecDeque::new(),
      pending_system_audio: VecDeque::new(),
      rejection_streak: 0,
      stats,
      system_audio_end_ns: None,
      system_audio_input,
      tail: None,
      timeline: Timeline::default(),
      width,
      writer,
    })
  }

  /// A moment on the writer's own monotonic clock, in nanoseconds.
  fn elapsed_ns(&self, at: Instant) -> i64 {
    i64::try_from(at.saturating_duration_since(self.base).as_nanos()).unwrap_or(i64::MAX)
  }

  pub(super) fn run(
    mut self,
    commands: &Receiver<Command>,
    first_frame: &mpsc::Sender<Result<(), String>>,
  ) {
    let mut announced = false;

    while let Ok(command) = commands.recv() {
      match command {
        Command::Frame(frame) => {
          if self.timeline.is_paused() {
            // Still worth keeping: it is the frame the movie resumes from and
            // the one the poster is drawn from if the user stops here.
            self.tail = Some(frame);
            continue;
          }

          let is_first_frame = !self.timeline.has_started();
          let origin_source_ns = frame.source_ns;
          if is_first_frame {
            self.origin_source_ns = Some(origin_source_ns);
            self.origin_wall = Some(frame.wall);
          }
          let pts = self
            .timeline
            .frame_pts_ns(frame.source_ns, self.elapsed_ns(frame.wall));
          let appended = self.append(&frame, pts);
          self.tail = Some(frame);

          if is_first_frame {
            self.flush_system_audio_preroll();
            self.flush_microphone_preroll();
          }

          if !announced && appended {
            announced = true;
            let _ = first_frame.send(Ok(()));
          }
        }
        Command::SystemAudio(sample) => {
          if !self.timeline.has_started() {
            if self.pending_system_audio.len() == SYSTEM_AUDIO_PREROLL_LIMIT {
              self.pending_system_audio.pop_front();
            }
            self.pending_system_audio.push_back(sample);
          } else if !self.timeline.is_paused() {
            self.append_system_audio_from_origin(sample);
          }
        }
        Command::Microphone(microphone) => {
          if !self.timeline.has_started() {
            if self.pending_microphone.len() == MICROPHONE_PREROLL_LIMIT {
              self.pending_microphone.pop_front();
            }
            self.pending_microphone.push_back(microphone);
          } else if !self.timeline.is_paused() {
            self.append_microphone_from_origin(microphone);
          }
        }
        Command::MicrophoneFailed(error) => {
          if !self.microphone_failure_reported {
            self.microphone_failure_reported = true;
            eprintln!("Microphone capture failed: {error}");
            (self.on_failure)(format!("The microphone stopped recording: {error}"));
          }
        }
        Command::Pause { at } => self.timeline.pause(self.elapsed_ns(at)),
        Command::Resume { at } => self.timeline.resume(self.elapsed_ns(at)),
        Command::Stop { at, reply } => {
          let _ = reply.send(self.finish(at));
          return;
        }
        Command::Cancel => {
          self.writer.cancel_writing();
          return;
        }
      }
    }

    // The session outlived its controller, which only happens if the handle
    // was dropped without stopping. Leave nothing half-written behind.
    self.writer.cancel_writing();
  }
}
