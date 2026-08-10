// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// A captured frame on its way to the writer thread.
pub(super) struct Frame {
  pub(super) buf: arc::R<cv::PixelBuf>,
  pub(super) clock: FrameClock,
  pub(super) wall: Instant,
}

#[derive(Clone, Copy)]
pub(super) enum FrameClock {
  Source(i64),
  Wall,
}

// SAFETY: a `cv::PixelBuf` is not `Send` because nothing stops two threads
// using one at once. Here the channel serialises it: the capture callback
// retains the buffer, moves it into the channel, and never looks at it again,
// and the writer thread is the only other holder. Ownership is handed over
// exactly once, which is the guarantee `Send` asks for.
unsafe impl Send for Frame {}

/// A ScreenCaptureKit PCM buffer on its way to the system-audio track.
pub(super) struct AudioSample {
  pub(super) buf: arc::R<cm::SampleBuf>,
  pub(super) source_ns: i64,
  pub(super) wall: Instant,
}

pub(super) enum SystemAudioSample {
  Pcm(MicrophoneBuffer),
  ScreenCaptureKit(AudioSample),
}

/// Everything the writer thread reacts to, in the order it happened.
pub(super) enum Command {
  Frame(Frame),
  SystemAudio(SystemAudioSample),
  Microphone(MicrophoneBuffer),
  MicrophoneFailed(String),
  Pause {
    at: Instant,
  },
  Resume {
    at: Instant,
  },
  Stop {
    at: Instant,
    reply: mpsc::Sender<Result<FinalizeInfo, String>>,
  },
  Cancel,
}

/// Counters shared by the capture callback and the writer thread, so a
/// recording can say afterwards how much of it never reached the file.
#[derive(Default)]
pub(super) struct CaptureStats {
  pub(super) appended: AtomicU64,
  /// Frames AVFoundation discarded before invoking the camera callback.
  pub(super) capture_dropped: AtomicU64,
  /// Frames the capture callback could not hand over, because the writer was
  /// still busy with the ones before them.
  pub(super) dropped: AtomicU64,
  /// Frames the encoder was not ready for. Expected in small numbers.
  pub(super) not_ready: AtomicU64,
  /// Frames the writer refused. Any of these means something is wrong.
  pub(super) rejected: AtomicU64,
  pub(super) audio_dropped: AtomicU64,
  pub(super) audio_not_ready: AtomicU64,
  pub(super) audio_rejected: AtomicU64,
  pub(super) microphone_dropped: AtomicU64,
  pub(super) microphone_not_ready: AtomicU64,
  pub(super) microphone_rejected: AtomicU64,
}

#[repr(C)]
pub(super) struct ScreenOutputInner {
  pub(super) commands: SyncSender<Command>,
  pub(super) stats: Arc<CaptureStats>,
}

impl ScreenOutputInner {
  fn handle_video(&mut self, sample: &cm::SampleBuf) {
    // ScreenCaptureKit sends a frame on every change *and* status-only frames
    // when the screen goes idle, gets blanked or is suspended. Only a complete
    // one carries pixels worth writing.
    if frame_status(sample) != Some(sc::FrameStatus::Complete) {
      return;
    }
    let Some(image) = sample.image_buf() else {
      return;
    };
    let Some(source_ns) = time_to_ns(sample.pts()) else {
      return;
    };

    let frame = Frame {
      buf: image.retained(),
      clock: FrameClock::Source(source_ns),
      wall: Instant::now(),
    };
    // Never blocks. A full channel means the writer is behind, and stalling
    // this callback would stall the window server's capture path with it.
    if let Err(TrySendError::Full(_)) = self.commands.try_send(Command::Frame(frame)) {
      self.stats.dropped.fetch_add(1, Ordering::Relaxed);
    }
  }

  fn handle_system_audio(&mut self, sample: &cm::SampleBuf) {
    if !sample.data_is_ready() {
      return;
    }
    let Some(source_ns) = time_to_ns(sample.pts()) else {
      return;
    };
    let audio = AudioSample {
      buf: sample.retained(),
      source_ns,
      wall: Instant::now(),
    };
    if let Err(TrySendError::Full(_)) =
      self
        .commands
        .try_send(Command::SystemAudio(SystemAudioSample::ScreenCaptureKit(
          audio,
        )))
    {
      self.stats.audio_dropped.fetch_add(1, Ordering::Relaxed);
    }
  }
}

define_obj_type!(
  pub(super) ScreenOutput + OutputImpl,
  ScreenOutputInner,
  SCREEN_OUTPUT_CLS
);

impl Output for ScreenOutput {}

#[objc::add_methods]
impl OutputImpl for ScreenOutput {
  extern "C" fn impl_stream_did_output_sample_buf(
    &mut self,
    _command: Option<&objc::Sel>,
    _stream: &sc::Stream,
    sample_buffer: &mut cm::SampleBuf,
    kind: sc::OutputType,
  ) {
    match kind {
      sc::OutputType::Screen => self.inner_mut().handle_video(sample_buffer),
      sc::OutputType::Audio => self.inner_mut().handle_system_audio(sample_buffer),
      _ => {}
    }
  }
}
