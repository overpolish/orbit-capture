#![allow(clippy::useless_transmute)]

//! Screen recording on macOS: ScreenCaptureKit into AVAssetWriter, H.264, no
//! intermediate files and no ffmpeg.
//!
//! # Who owns what
//!
//! `av::AssetWriter`, its input and the pixel buffer adaptor are not `Send`,
//! so they are created on, live on, and die on a single dedicated writer
//! thread that owns them for the whole recording. Nothing else ever touches
//! them. That thread's only input is one bounded channel.
//!
//! ScreenCaptureKit delivers frames on a dispatch queue. That callback does
//! the least work it possibly can - check the frame is a real one, retain its
//! pixel buffer, and hand it to the channel with `try_send`. It never blocks:
//! when the writer is behind, the channel is full and the frame is counted as
//! dropped rather than stalling the capture, which is what would make the
//! whole machine stutter.
//!
//! Pause, resume, stop and cancel travel down that same channel, so they are
//! ordered against the frames for free. There is no lock anywhere in the hot
//! path, and no state that two threads can see at once.

use std::collections::VecDeque;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cidre::{
  arc, av, cat, cf, cg, cm, cv, define_obj_type, dispatch, ns, objc, os, sc,
  sc::stream::{Output, OutputImpl},
};
use cpal::Stream;

use crate::capture_kit::{application_audio_filter, monitor_geometry, our_windows};

use super::encoding::{
  bitrate_bps, nv12_poster_rgba, poster_size, FailureReport, FinalizeInfo, Plane, Timeline,
};
use super::microphone::{
  Buffer as MicrophoneBuffer, Format as MicrophoneFormat, Source as MicrophoneSource,
};
use super::SystemAudioSelection;

/// How many frames ScreenCaptureKit may have in flight for us.
const STREAM_QUEUE_DEPTH: isize = 8;
/// How many frames may be waiting on the writer thread. Deeper than this only
/// buys latency: a backlog the writer cannot clear is a dropped frame either
/// way, and dropping it early keeps memory flat.
const FRAME_QUEUE_DEPTH: usize = 8;
const NANOS_PER_SEC: i64 = 1_000_000_000;
const NANOS_PER_MS: i64 = 1_000_000;
/// The long edge of the poster shipped to the export window.
const POSTER_MAX_EDGE: u32 = 640;
/// Finishing writes out the movie's index, which is fast but not instant.
const FINALIZE_TIMEOUT: Duration = Duration::from_secs(30);
/// How many frames in a row the writer may refuse before the user is told.
/// A handful of refusals is ordinary back-pressure; a run this long means the
/// recording is not going to recover on its own.
const REJECTION_STREAK_LIMIT: u64 = 60;
/// How hard the closing frame is pressed on a busy encoder, and how long
/// between tries. Half a second in total, which is not long enough to be felt
/// when stopping and is far longer than a real encoder ever needs.
const TAIL_APPEND_ATTEMPTS: u32 = 50;
const TAIL_APPEND_WAIT: Duration = Duration::from_millis(10);
const SYSTEM_AUDIO_SAMPLE_RATE: i64 = 48_000;
const SYSTEM_AUDIO_CHANNELS: i64 = 2;
const SYSTEM_AUDIO_BITRATE: i32 = 192_000;
const MICROPHONE_AUDIO_BITRATE: i32 = 128_000;
/// More than a second of audio at ScreenCaptureKit's usual 1024-frame buffer
/// size. In practice the first video frame arrives within one or two buffers;
/// the bound only prevents a broken display stream retaining audio forever.
const SYSTEM_AUDIO_PREROLL_LIMIT: usize = 64;
const MICROPHONE_PREROLL_LIMIT: usize = 64;

/// A captured frame on its way to the writer thread.
struct Frame {
  buf: arc::R<cv::PixelBuf>,
  source_ns: i64,
  wall: Instant,
}

// SAFETY: a `cv::PixelBuf` is not `Send` because nothing stops two threads
// using one at once. Here the channel serialises it: the capture callback
// retains the buffer, moves it into the channel, and never looks at it again,
// and the writer thread is the only other holder. Ownership is handed over
// exactly once, which is the guarantee `Send` asks for.
unsafe impl Send for Frame {}

/// A ScreenCaptureKit PCM buffer on its way to the system-audio track.
struct AudioSample {
  buf: arc::R<cm::SampleBuf>,
  source_ns: i64,
  wall: Instant,
}

/// Everything the writer thread reacts to, in the order it happened.
enum Command {
  Frame(Frame),
  SystemAudio(AudioSample),
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
struct CaptureStats {
  appended: AtomicU64,
  /// Frames the capture callback could not hand over, because the writer was
  /// still busy with the ones before them.
  dropped: AtomicU64,
  /// Frames the encoder was not ready for. Expected in small numbers.
  not_ready: AtomicU64,
  /// Frames the writer refused. Any of these means something is wrong.
  rejected: AtomicU64,
  audio_dropped: AtomicU64,
  audio_not_ready: AtomicU64,
  audio_rejected: AtomicU64,
  microphone_dropped: AtomicU64,
  microphone_not_ready: AtomicU64,
  microphone_rejected: AtomicU64,
}

#[repr(C)]
struct ScreenOutputInner {
  commands: SyncSender<Command>,
  stats: Arc<CaptureStats>,
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
      source_ns,
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
    if let Err(TrySendError::Full(_)) = self.commands.try_send(Command::SystemAudio(audio)) {
      self.stats.audio_dropped.fetch_add(1, Ordering::Relaxed);
    }
  }
}

define_obj_type!(
  ScreenOutput + OutputImpl,
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

/// Reads the frame status ScreenCaptureKit attaches to every sample.
///
/// The attachment dictionary is keyed by `cf::String` while the constant is an
/// `ns::String`, which is why the key is bridged rather than used directly.
fn frame_status(sample: &cm::SampleBuf) -> Option<sc::FrameStatus> {
  let attachments = sample.attaches(false)?;
  if attachments.is_empty() {
    return None;
  }
  let raw = attachments[0]
    .get(sc::FrameInfo::status().as_cf())?
    .try_as_number()?
    .to_i64()?;

  match raw {
    0 => Some(sc::FrameStatus::Complete),
    1 => Some(sc::FrameStatus::Idle),
    2 => Some(sc::FrameStatus::Blank),
    3 => Some(sc::FrameStatus::Suspended),
    4 => Some(sc::FrameStatus::Started),
    5 => Some(sc::FrameStatus::Stopped),
    _ => None,
  }
}

/// A `cm::Time` in nanoseconds, or `None` if it is not a real timestamp.
fn time_to_ns(time: cm::Time) -> Option<i64> {
  if time.scale <= 0 || !time.flags.contains(cm::TimeFlags::VALID) {
    return None;
  }
  let nanos = i128::from(time.value) * i128::from(NANOS_PER_SEC) / i128::from(time.scale);

  i64::try_from(nanos).ok()
}

fn nanos(time: i64) -> cm::Time {
  cm::Time::new(time, NANOS_PER_SEC as cm::TimeScale)
}

/// H.264 subsamples chroma, so an odd edge has nowhere to put half a pixel.
/// The stream and the encoder are both configured with the rounded size so
/// they can never disagree about what a frame is.
const fn even(value: u32) -> u32 {
  value & !1
}

/// The encoder settings for one recording.
fn video_settings(
  width: u32,
  height: u32,
  fps: u32,
) -> arc::R<ns::DictionaryMut<ns::String, ns::Id>> {
  // These keys have no bound constants, so they are spelled out the way
  // cidre's own examples do.
  let bitrate = ns::Number::with_i32(bitrate_bps(width, height, fps));
  let expected_frame_rate = ns::Number::with_i32(fps as i32);
  // A keyframe every two seconds: often enough to scrub, rare enough not to
  // spend the bitrate on it.
  let max_key_frame_interval = ns::Number::with_i32((fps * 2) as i32);

  let mut compression = ns::DictionaryMut::<ns::String, ns::Id>::with_capacity(4);
  compression.insert(ns::str!(c"AverageBitRate"), &bitrate);
  compression.insert(ns::str!(c"ExpectedFrameRate"), &expected_frame_rate);
  compression.insert(ns::str!(c"MaxKeyFrameInterval"), &max_key_frame_interval);
  compression.insert(ns::str!(c"ProfileLevel"), ns::str!(c"H264_High_AutoLevel"));

  let width = ns::Number::with_i32(width as i32);
  let height = ns::Number::with_i32(height as i32);
  let mut settings = ns::DictionaryMut::<ns::String, ns::Id>::with_capacity(4);
  settings.insert(av::video_settings_keys::codec(), av::VideoCodec::h264());
  settings.insert(av::video_settings_keys::width(), &width);
  settings.insert(av::video_settings_keys::height(), &height);
  settings.insert(av::video_settings_keys::compression_props(), &compression);

  settings
}

fn audio_settings(
  sample_rate: i64,
  channels: i64,
  bitrate: i32,
) -> arc::R<ns::DictionaryMut<ns::String, ns::Id>> {
  let sample_rate = ns::Number::with_i64(sample_rate);
  let channels = ns::Number::with_i64(channels);
  let bitrate = ns::Number::with_i32(bitrate);
  let mut settings = ns::DictionaryMut::<ns::String, ns::Id>::with_capacity(4);
  settings.insert(
    av::audio::all_formats_keys::id(),
    cat::AudioFormat::MPEG4_AAC.as_ref(),
  );
  settings.insert(av::audio::all_formats_keys::sample_rate(), &sample_rate);
  settings.insert(av::audio::all_formats_keys::number_of_channels(), &channels);
  settings.insert(
    av::audio::settings::encoder_propery_keys::bit_rate(),
    &bitrate,
  );

  settings
}

fn system_audio_settings() -> arc::R<ns::DictionaryMut<ns::String, ns::Id>> {
  audio_settings(
    SYSTEM_AUDIO_SAMPLE_RATE,
    SYSTEM_AUDIO_CHANNELS,
    SYSTEM_AUDIO_BITRATE,
  )
}

fn microphone_audio_settings(
  format: MicrophoneFormat,
) -> arc::R<ns::DictionaryMut<ns::String, ns::Id>> {
  audio_settings(
    i64::from(format.sample_rate),
    i64::from(format.channels),
    MICROPHONE_AUDIO_BITRATE,
  )
}

unsafe extern "C" {
  fn CMSampleBufferCreateCopyWithNewTiming(
    allocator: Option<&cf::Allocator>,
    original: &cm::SampleBuf,
    timing_count: cm::ItemCount,
    timing: *const cm::SampleTimingInfo,
    output: *mut Option<arc::R<cm::SampleBuf>>,
  ) -> os::Status;

  fn CMSampleBufferCopySampleBufferForRange(
    allocator: Option<&cf::Allocator>,
    original: &cm::SampleBuf,
    range: cf::Range,
    output: *mut Option<arc::R<cm::SampleBuf>>,
  ) -> os::Status;
}

/// Keeps only the part of a PCM buffer at or after the movie origin.
///
/// ScreenCaptureKit normally delivers audio before its first video frame. The
/// first video frame remains time zero, but retaining the overlapping portion
/// of that audio buffer means the AAC track begins at zero as well instead of
/// waiting for the next 21 ms buffer boundary.
fn audio_sample_from_origin(sample: AudioSample, origin_source_ns: i64) -> Option<AudioSample> {
  let delta_ns = origin_source_ns.saturating_sub(sample.source_ns);
  if delta_ns <= 0 {
    return Some(sample);
  }

  let sample_count = sample.buf.num_samples();
  if sample_count <= 0 {
    return None;
  }
  let duration_ns = time_to_ns(sample.buf.duration())?;
  if duration_ns <= 0 || delta_ns >= duration_ns {
    return None;
  }

  // Round up: the retained first PCM frame must not precede the video origin.
  let trim = ((i128::from(delta_ns) * sample_count as i128 + i128::from(duration_ns) - 1)
    / i128::from(duration_ns))
  .clamp(0, sample_count as i128) as cf::Index;
  if trim >= sample_count {
    return None;
  }

  let mut output = None;
  unsafe {
    CMSampleBufferCopySampleBufferForRange(
      None,
      &sample.buf,
      cf::Range::new(trim, sample_count - trim),
      &mut output,
    )
  }
  .result()
  .ok()?;
  let buf = output?;
  let source_ns = time_to_ns(buf.pts()).unwrap_or(origin_source_ns);

  Some(AudioSample {
    buf,
    source_ns: source_ns.max(origin_source_ns),
    wall: sample.wall,
  })
}

fn audio_sample_with_pts(
  sample: &cm::SampleBuf,
  pts_ns: i64,
) -> Result<arc::R<cm::SampleBuf>, String> {
  let original = sample.timing_info(0).map_err(|error| error.to_string())?;
  let timing = cm::SampleTimingInfo {
    duration: original.duration,
    pts: nanos(pts_ns),
    dts: cm::Time::invalid(),
  };
  let mut output = None;
  unsafe { CMSampleBufferCreateCopyWithNewTiming(None, sample, 1, &timing, &mut output) }
    .result()
    .map_err(|error| error.to_string())?;
  output.ok_or_else(|| "The system-audio sample could not be retimed".to_owned())
}

fn microphone_format_description(
  format: MicrophoneFormat,
) -> Result<arc::R<cm::AudioFormatDesc>, String> {
  let bytes_per_frame = u32::from(format.channels) * size_of::<f32>() as u32;
  let description = cat::audio::StreamBasicDesc {
    sample_rate: f64::from(format.sample_rate),
    format: cat::AudioFormat::LINEAR_PCM,
    format_flags: cat::audio::FormatFlags::NATIVE_FLOAT_PACKED,
    bytes_per_packet: bytes_per_frame,
    frames_per_packet: 1,
    bytes_per_frame,
    channels_per_frame: u32::from(format.channels),
    bits_per_channel: 32,
    reserved: 0,
  };
  cm::AudioFormatDesc::with_asbd(&description).map_err(|error| error.to_string())
}

fn microphone_sample_buffer(
  microphone: &MicrophoneBuffer,
  format: MicrophoneFormat,
  description: &cm::AudioFormatDesc,
  pts_ns: i64,
) -> Result<arc::R<cm::SampleBuf>, String> {
  let channels = usize::from(format.channels);
  if channels == 0 {
    return Err("The microphone reported no channels".to_owned());
  }
  let frames = microphone.samples.len() / channels;
  if frames == 0 {
    return Err("The microphone produced an empty buffer".to_owned());
  }
  let sample_count = frames * channels;
  let byte_count = sample_count * size_of::<f32>();
  let mut data = cm::BlockBuf::with_mem_block(byte_count).map_err(|error| error.to_string())?;
  let source =
    unsafe { std::slice::from_raw_parts(microphone.samples.as_ptr().cast::<u8>(), byte_count) };
  data
    .as_mut_slice()
    .map_err(|error| error.to_string())?
    .copy_from_slice(source);

  let timing = cm::SampleTimingInfo {
    duration: cm::Time::new(1, format.sample_rate as cm::TimeScale),
    pts: nanos(pts_ns),
    dts: cm::Time::invalid(),
  };
  let sample_size = channels * size_of::<f32>();
  let mut output = None;
  unsafe {
    cm::SampleBuf::create_in(
      None,
      Some(&data),
      true,
      None,
      std::ptr::null(),
      Some(description),
      frames as cm::ItemCount,
      1,
      &timing,
      1,
      &sample_size,
      &mut output,
    )
  }
  .map_err(|error| error.to_string())?;
  output.ok_or_else(|| "The microphone sample could not be created".to_owned())
}

fn microphone_buffer_from_origin(
  mut microphone: MicrophoneBuffer,
  origin: Instant,
  format: MicrophoneFormat,
) -> Option<MicrophoneBuffer> {
  let channels = usize::from(format.channels);
  if channels == 0 {
    return None;
  }
  let frames = microphone.samples.len() / channels;
  if frames == 0 {
    return None;
  }
  let Some(before_origin) = origin.checked_duration_since(microphone.captured_at) else {
    return Some(microphone);
  };
  if before_origin.is_zero() {
    return Some(microphone);
  }
  let trim_frames = (before_origin.as_nanos() * u128::from(format.sample_rate))
    .div_ceil(NANOS_PER_SEC as u128)
    .min(frames as u128) as usize;
  if trim_frames >= frames {
    return None;
  }

  microphone.samples = microphone.samples.split_off(trim_frames * channels);
  let trimmed_ns = (trim_frames as u128 * NANOS_PER_SEC as u128 / u128::from(format.sample_rate))
    .min(u128::from(u64::MAX)) as u64;
  microphone.captured_at += Duration::from_nanos(trimmed_ns);
  Some(microphone)
}

/// The writer thread's whole world. Created on that thread, dropped on it.
struct Writer {
  adaptor: arc::R<av::asset::WriterInputPixelBufAdaptor>,
  base: Instant,
  height: u32,
  input: arc::R<av::AssetWriterInput>,
  system_audio_input: Option<arc::R<av::AssetWriterInput>>,
  last_system_audio_pts_ns: Option<i64>,
  microphone_input: Option<arc::R<av::AssetWriterInput>>,
  microphone_format: Option<MicrophoneFormat>,
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
  on_failure: FailureReport,
  path: PathBuf,
  pending_microphone: VecDeque<MicrophoneBuffer>,
  pending_system_audio: VecDeque<AudioSample>,
  rejection_streak: u64,
  stats: Arc<CaptureStats>,
  /// The last frame seen, appended once more at the true stop time. Without
  /// it a recording of a screen that stopped changing ends at its last change
  /// rather than when the user stopped it.
  tail: Option<Frame>,
  timeline: Timeline,
  width: u32,
  writer: arc::R<av::AssetWriter>,
}

struct WriterConfig {
  path: PathBuf,
  width: u32,
  height: u32,
  fps: u32,
  system_audio: bool,
  microphone_format: Option<MicrophoneFormat>,
  stats: Arc<CaptureStats>,
  on_failure: FailureReport,
  /// Always `Container::quicktime_fragmented()` in the app. It is a field
  /// rather than a constant so the encoder tests can drive a real writer at
  /// the containers that were rejected and keep proving why.
  container: Container,
}

/// What shape of file the writer produces, and how often - if ever - it
/// flushes a movie fragment to disk.
#[derive(Clone, Copy)]
struct Container {
  format: ContainerFormat,
  /// `None` leaves the index to be written in one go at `finish_writing`, so
  /// the file is worthless until then. `Some` asks for a self-contained
  /// fragment this often, which is what would make a half-written recording
  /// playable - at the cost documented in `Writer::new`.
  fragment_interval: Option<cm::Time>,
}

/// Named rather than held as an `&'static av::FileType` because the config
/// travels to the writer thread, and Objective-C strings are not `Send`. The
/// real file type is fetched once the config has arrived.
#[derive(Clone, Copy)]
enum ContainerFormat {
  /// Reachable only from the encoder tests now. It is what recordings used to
  /// be written as, and the tests keep it around as the control the fragmented
  /// QuickTime container is measured against.
  #[cfg(test)]
  Mp4,
  /// What recordings are written as. Movie fragments are a QuickTime feature
  /// that .mp4 merely borrows, and only QuickTime survives being fragmented -
  /// see [`Container::quicktime_fragmented`].
  QuickTime,
}

impl ContainerFormat {
  fn file_type(self) -> &'static av::FileType {
    match self {
      #[cfg(test)]
      Self::Mp4 => av::FileType::mp4(),
      Self::QuickTime => av::FileType::qt(),
    }
  }

  /// AVFoundation infers nothing from the URL, but everything that opens the
  /// file afterwards reads the name, so the two have to agree. Production
  /// spells the working file's name out in `encoding::temp_file_name` instead,
  /// because that name is built on every platform and this type is macOS's;
  /// `names_the_working_file_after_the_container_it_is` holds the two together.
  #[cfg(test)]
  fn extension(self) -> &'static str {
    match self {
      #[cfg(test)]
      Self::Mp4 => "mp4",
      Self::QuickTime => "mov",
    }
  }
}

/// How often a fragment is flushed. Two seconds is the most a crash can cost,
/// and short enough that the overhead of a fragment header never shows up
/// against a screen recording's bitrate. The timescale is the 600 QuickTime
/// has always used for durations.
const FRAGMENT_INTERVAL_SECONDS: f64 = 2.0;
const FRAGMENT_TIMESCALE: i32 = 600;

impl Container {
  /// What recordings are written as: a QuickTime movie that stamps a
  /// self-contained fragment every two seconds.
  ///
  /// The point is that a recording is worth something before it is finished.
  /// An unfragmented file has its index written in one go at
  /// `finish_writing`, so a crash - or a kill, or a panic - leaves a corpse
  /// with no `moov` atom at all, which no player and no repair tool can make
  /// anything of. Fragmented, the same interruption leaves a movie that probes
  /// and decodes cleanly up to the last flushed fragment.
  ///
  /// The container has to be QuickTime. Fragmenting an .mp4 through this
  /// pipeline puts the writer into a failed state - sometimes at the first
  /// fragment boundary after a resume, so every later frame is refused, and
  /// sometimes only at `finish_writing`, which loses the whole recording. The
  /// ignored encoder tests at the foot of this file hold both halves of that
  /// evidence: the fragmented QuickTime corpse of an aborted ten-second
  /// recording probes at 8.02s and decodes 243 frames without an error, while
  /// its .mp4 twin is `moov atom not found`.
  ///
  /// One caveat travels with fragments: `nb_frames` in the finished header
  /// counts only the frames of the last fragment - 61 against the 243 that
  /// actually decode - so nothing may read a frame count out of the container.
  /// Durations stay exact, which is what everything here uses anyway.
  ///
  /// The saved file is still an .mp4: see `exports::save_recording`, which
  /// stream-copies the working movie into one rather than renaming it.
  fn quicktime_fragmented() -> Self {
    Self {
      format: ContainerFormat::QuickTime,
      fragment_interval: Some(cm::Time::with_secs(
        FRAGMENT_INTERVAL_SECONDS,
        FRAGMENT_TIMESCALE,
      )),
    }
  }

  /// The container recordings used to be written as, kept for the encoder
  /// tests that measure the fragmented one against it.
  #[cfg(test)]
  const fn mp4() -> Self {
    Self {
      format: ContainerFormat::Mp4,
      fragment_interval: None,
    }
  }
}

impl Writer {
  fn new(config: WriterConfig) -> Result<Self, String> {
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

  fn run(mut self, commands: &Receiver<Command>, first_frame: &mpsc::Sender<Result<(), String>>) {
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

  fn flush_system_audio_preroll(&mut self) {
    let pending = std::mem::take(&mut self.pending_system_audio);
    for sample in pending {
      self.append_system_audio_from_origin(sample);
    }
  }

  fn append_system_audio_from_origin(&mut self, sample: AudioSample) {
    let Some(origin_source_ns) = self.origin_source_ns else {
      return;
    };
    if let Some(sample) = audio_sample_from_origin(sample, origin_source_ns) {
      self.append_mapped_system_audio(&sample);
    }
  }

  fn append_mapped_system_audio(&mut self, sample: &AudioSample) {
    let mut pts = self
      .timeline
      .media_pts_ns(sample.source_ns, self.elapsed_ns(sample.wall));
    if let Some(last) = self.last_system_audio_pts_ns {
      pts = pts.max(last.saturating_add(1));
    }
    self.append_system_audio(sample, pts);
  }

  fn flush_microphone_preroll(&mut self) {
    let pending = std::mem::take(&mut self.pending_microphone);
    for microphone in pending {
      self.append_microphone_from_origin(microphone);
    }
  }

  fn append_microphone_from_origin(&mut self, microphone: MicrophoneBuffer) {
    let (Some(origin), Some(format)) = (self.origin_wall, self.microphone_format) else {
      return;
    };
    let Some(microphone) = microphone_buffer_from_origin(microphone, origin, format) else {
      return;
    };
    let mut pts = self
      .timeline
      .wall_pts_ns(self.elapsed_ns(microphone.captured_at));
    if let Some(last) = self.last_microphone_pts_ns {
      pts = pts.max(last.saturating_add(1));
    }
    self.append_microphone(&microphone, format, pts);
  }

  /// Appends one frame, reporting whether it actually landed.
  ///
  /// A writer that has failed is never asked again: AVFoundation refuses every
  /// subsequent sample, and asking it sixty times a second turns one fault
  /// into a flood of identical complaints.
  fn append(&mut self, frame: &Frame, pts_ns: i64) -> bool {
    if self.failed.is_some() {
      self.stats.rejected.fetch_add(1, Ordering::Relaxed);
      return false;
    }
    if !self.input.is_ready_for_more_media_data() {
      self.stats.not_ready.fetch_add(1, Ordering::Relaxed);
      return false;
    }

    match self
      .adaptor
      .append_pixel_buf_with_pts(&frame.buf, nanos(pts_ns))
    {
      Ok(true) => {
        self.stats.appended.fetch_add(1, Ordering::Relaxed);
        self.rejection_streak = 0;
        self.last_appended_ns = Some(pts_ns);
        true
      }
      Ok(false) => {
        self.refused(writer_error(
          &self.writer,
          "the recording could not continue",
        ));
        false
      }
      Err(error) => {
        self.refused(error.to_string());
        false
      }
    }
  }

  fn append_system_audio(&mut self, sample: &AudioSample, pts_ns: i64) {
    if self.failed.is_some() || self.system_audio_input.is_none() {
      return;
    }
    if !self
      .system_audio_input
      .as_ref()
      .is_some_and(|input| input.is_ready_for_more_media_data())
    {
      self.stats.audio_not_ready.fetch_add(1, Ordering::Relaxed);
      return;
    }

    let retimed = match audio_sample_with_pts(&sample.buf, pts_ns) {
      Ok(sample) => sample,
      Err(error) => {
        self.stats.audio_rejected.fetch_add(1, Ordering::Relaxed);
        self.refused(error);
        return;
      }
    };
    let duration_ns = time_to_ns(sample.buf.duration()).unwrap_or_default();
    let result = self
      .system_audio_input
      .as_mut()
      .expect("checked above")
      .append_sample_buf(&retimed);
    match result {
      Ok(true) => {
        self.last_system_audio_pts_ns = Some(pts_ns);
        self.system_audio_end_ns = Some(pts_ns.saturating_add(duration_ns));
      }
      Ok(false) => {
        self.stats.audio_rejected.fetch_add(1, Ordering::Relaxed);
        self.refused(writer_error(
          &self.writer,
          "the system-audio track could not continue",
        ));
      }
      Err(error) => {
        self.stats.audio_rejected.fetch_add(1, Ordering::Relaxed);
        self.refused(error.to_string());
      }
    }
  }

  fn append_microphone(
    &mut self,
    microphone: &MicrophoneBuffer,
    format: MicrophoneFormat,
    pts_ns: i64,
  ) {
    if self.failed.is_some() || self.microphone_input.is_none() {
      return;
    }
    if !self
      .microphone_input
      .as_ref()
      .is_some_and(|input| input.is_ready_for_more_media_data())
    {
      self
        .stats
        .microphone_not_ready
        .fetch_add(1, Ordering::Relaxed);
      return;
    }

    let Some(description) = self.microphone_format_description.as_ref() else {
      return;
    };
    let sample = match microphone_sample_buffer(microphone, format, description, pts_ns) {
      Ok(sample) => sample,
      Err(error) => {
        self
          .stats
          .microphone_rejected
          .fetch_add(1, Ordering::Relaxed);
        self.refused(error);
        return;
      }
    };
    let duration_ns = time_to_ns(sample.duration()).unwrap_or_default();
    let result = self
      .microphone_input
      .as_mut()
      .expect("checked above")
      .append_sample_buf(&sample);
    match result {
      Ok(true) => {
        self.last_microphone_pts_ns = Some(pts_ns);
        self.microphone_end_ns = Some(pts_ns.saturating_add(duration_ns));
      }
      Ok(false) => {
        self
          .stats
          .microphone_rejected
          .fetch_add(1, Ordering::Relaxed);
        self.refused(writer_error(
          &self.writer,
          "the microphone track could not continue",
        ));
      }
      Err(error) => {
        self
          .stats
          .microphone_rejected
          .fetch_add(1, Ordering::Relaxed);
        self.refused(error.to_string());
      }
    }
  }

  /// Appends the closing frame, giving a busy encoder a moment to catch up.
  ///
  /// Ordinary frames are dropped the instant the encoder is busy - there is
  /// another one along in sixteen milliseconds. This one is the last there
  /// will ever be, and the movie's length depends on it landing.
  fn append_insisting(&mut self, frame: &Frame, pts_ns: i64) {
    for _ in 0..TAIL_APPEND_ATTEMPTS {
      if self.append(frame, pts_ns) || self.failed.is_some() {
        return;
      }
      std::thread::sleep(TAIL_APPEND_WAIT);
    }
  }

  /// Records a refused frame, and tells the user once if they have stopped
  /// landing altogether.
  fn refused(&mut self, reason: String) {
    self.stats.rejected.fetch_add(1, Ordering::Relaxed);
    self.rejection_streak += 1;

    // A failed writer never recovers, so there is no point waiting out the
    // streak before saying so. Short of that, a long enough run of refusals
    // means the same thing by another route.
    let hopeless = self.writer.status() == av::AssetWriterStatus::Failed
      || self.rejection_streak >= REJECTION_STREAK_LIMIT;
    if hopeless {
      self.fail(reason);
    }
  }

  /// Latches the failure and reports it exactly once.
  fn fail(&mut self, reason: String) {
    if self.failed.is_some() {
      return;
    }
    eprintln!("Recording stopped accepting frames: {reason}");
    (self.on_failure)(reason.clone());
    self.failed = Some(reason);
  }

  fn finish(&mut self, at: Instant) -> Result<FinalizeInfo, String> {
    if !self.timeline.has_started() {
      self.writer.cancel_writing();
      return Err("The recording captured no frames".to_owned());
    }

    let stop_ns = self.timeline.stop_pts_ns(self.elapsed_ns(at));
    // Holding the final frame until the true stop time is what gives a
    // recording of a static screen its real duration, so a busy encoder is
    // worth waiting a moment for rather than giving up on.
    let mut tail = self.tail.take();
    if let Some(frame) = &tail {
      self.append_insisting(frame, stop_ns);
    }

    self.input.mark_as_finished();
    if let Some(input) = self.system_audio_input.as_mut() {
      input.mark_as_finished();
    }
    if let Some(input) = self.microphone_input.as_mut() {
      input.mark_as_finished();
    }
    // The session ends exactly where the media ends. Ending it any later
    // leaves the movie claiming a duration it has no sample to fill, and the
    // writer refuses the whole file for it - which is what a skipped final
    // frame used to cause, intermittently and only at the very end.
    let end_ns = self
      .last_appended_ns
      .unwrap_or(0)
      .max(self.system_audio_end_ns.unwrap_or(0))
      .max(self.microphone_end_ns.unwrap_or(0));
    self
      .writer
      .end_session_at_src_time(nanos(end_ns))
      .map_err(|error| error.to_string())?;
    self.writer.finish_writing();

    if self.writer.status() != av::AssetWriterStatus::Completed {
      return Err(writer_error(
        &self.writer,
        "The recording could not be saved",
      ));
    }

    let dropped = self.stats.dropped.load(Ordering::Relaxed);
    let not_ready = self.stats.not_ready.load(Ordering::Relaxed);
    if dropped > 0 || not_ready > 0 {
      eprintln!(
        "Recording dropped {dropped} frames at the capture queue and {not_ready} at the encoder"
      );
    }
    let audio_dropped = self.stats.audio_dropped.load(Ordering::Relaxed);
    let audio_not_ready = self.stats.audio_not_ready.load(Ordering::Relaxed);
    let audio_rejected = self.stats.audio_rejected.load(Ordering::Relaxed);
    if audio_dropped > 0 || audio_not_ready > 0 || audio_rejected > 0 {
      eprintln!(
        "Recording dropped {audio_dropped} system-audio buffers at the capture queue, {audio_not_ready} at the encoder, and rejected {audio_rejected}"
      );
    }
    let microphone_dropped = self.stats.microphone_dropped.load(Ordering::Relaxed);
    let microphone_not_ready = self.stats.microphone_not_ready.load(Ordering::Relaxed);
    let microphone_rejected = self.stats.microphone_rejected.load(Ordering::Relaxed);
    if microphone_dropped > 0 || microphone_not_ready > 0 || microphone_rejected > 0 {
      eprintln!(
        "Recording dropped {microphone_dropped} microphone buffers at the capture queue, {microphone_not_ready} at the encoder, and rejected {microphone_rejected}"
      );
    }

    Ok(FinalizeInfo {
      has_microphone: self.microphone_input.is_some(),
      has_system_audio: self.system_audio_input.is_some(),
      duration_ms: u64::try_from(end_ns / NANOS_PER_MS).unwrap_or_default(),
      height: self.height,
      path: self.path.clone(),
      poster: tail.as_mut().and_then(poster_png),
      // The recording state owns source geometry and fills this before the
      // artifact is presented. The writer itself only deals in pixels.
      source_scale_factor: 1.0,
      width: self.width,
    })
  }
}

/// The writer's own explanation, or the fallback when it has none.
fn writer_error(writer: &av::AssetWriter, fallback: &str) -> String {
  writer
    .error()
    .map_or_else(|| fallback.to_owned(), |error| error.to_string())
}

/// Draws the still shown in the export window from the recording's last frame.
fn poster_png(frame: &mut Frame) -> Option<Vec<u8>> {
  let buf = &mut *frame.buf;
  // A `420v` capture is always bi-planar; anything else is not a frame this
  // pipeline produced.
  if buf.plane_count() < 2 {
    return None;
  }
  let width = u32::try_from(buf.width()).ok()?;
  let height = u32::try_from(buf.height()).ok()?;
  let (out_width, out_height) = poster_size(width, height, POSTER_MAX_EDGE);

  let flags = cv::pixel_buffer::LockFlags::READ_ONLY;
  // SAFETY: the buffer stays locked for exactly the two reads below, and each
  // plane is bounded by the stride and height the buffer itself reports.
  unsafe { buf.lock_base_addr(flags) }.result().ok()?;
  let luma_stride = buf.plane_bytes_per_row(0);
  let chroma_stride = buf.plane_bytes_per_row(1);
  let luma_base = buf.plane_base_address(0);
  let chroma_base = buf.plane_base_address(1);
  let rgba = if luma_base.is_null() || chroma_base.is_null() {
    None
  } else {
    let luma = Plane {
      bytes: unsafe { std::slice::from_raw_parts(luma_base, luma_stride * buf.plane_height(0)) },
      stride: luma_stride,
    };
    let chroma = Plane {
      bytes: unsafe {
        std::slice::from_raw_parts(chroma_base, chroma_stride * buf.plane_height(1))
      },
      stride: chroma_stride,
    };
    Some(nv12_poster_rgba(
      luma, chroma, width, height, out_width, out_height,
    ))
  };
  unsafe { buf.unlock_lock_base_addr(flags) };

  let image = image::RgbaImage::from_raw(out_width, out_height, rgba?)?;
  let mut png = Vec::new();
  image::DynamicImage::ImageRgba8(image)
    .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
    .ok()?;

  Some(png)
}

/// The ScreenCaptureKit objects a running session keeps alive.
struct StreamObjects {
  _output: arc::R<ScreenOutput>,
  queue: arc::R<dispatch::Queue>,
  streams: Vec<arc::R<sc::Stream>>,
}

// SAFETY: `sc::Stream` already declares itself thread-safe. The queue is a
// dispatch object, which is thread-safe by construction. The output delegate's
// own state is only ever touched from the one serial queue it was registered
// with; every other thread does nothing to it but retain and release, which
// Objective-C makes atomic.
unsafe impl Send for StreamObjects {}

/// A running recording, as seen by the state machine.
pub struct CaptureSession {
  commands: SyncSender<Command>,
  microphone: Option<Stream>,
  objects: StreamObjects,
  worker: Option<JoinHandle<()>>,
}

impl CaptureSession {
  pub fn pause(&self) {
    let _ = self.commands.send(Command::Pause { at: Instant::now() });
  }

  pub fn resume(&self) -> Result<(), String> {
    self
      .commands
      .send(Command::Resume { at: Instant::now() })
      .map_err(|_| "The recording is no longer running".to_owned())
  }

  /// Finishes the movie and hands back what was written.
  ///
  /// The stop instant is taken before asking ScreenCaptureKit to stop, so the
  /// asynchronous shutdown time never lengthens the movie. Its completion is
  /// followed by a barrier on the serial output queue; only then is the writer
  /// finalized. That ordering guarantees the final audio buffers are written
  /// instead of being stranded behind `Stop`.
  pub fn stop(mut self) -> Result<FinalizeInfo, String> {
    let at = Instant::now();
    self.microphone.take();
    let (stopped, did_stop) = mpsc::channel();
    for stream in &self.objects.streams {
      let stopped = stopped.clone();
      stream.stop_with_ch(move |error| {
        let result = error.map_or_else(|| Ok(()), |error| Err(error.to_string()));
        let _ = stopped.send(result);
      });
    }
    drop(stopped);
    for _ in 0..self.objects.streams.len() {
      match did_stop.recv_timeout(FINALIZE_TIMEOUT) {
        Ok(Ok(())) => {}
        Ok(Err(error)) => eprintln!("ScreenCaptureKit reported an error while stopping: {error}"),
        Err(_) => {
          eprintln!("ScreenCaptureKit did not confirm shutdown before finalization");
          break;
        }
      }
    }
    self.objects.queue.sync_once(|| {});

    let (reply, replies) = mpsc::channel();
    self
      .commands
      .send(Command::Stop { at, reply })
      .map_err(|_| "The recording is no longer running".to_owned())?;
    let finalized = replies
      .recv_timeout(FINALIZE_TIMEOUT)
      .map_err(|_| "The recording did not finish in time".to_owned())?;
    self.join_writer();

    finalized
  }

  /// Throws the recording away. The file itself is deleted by the caller,
  /// which is the only place that knows whether it was ever wanted.
  pub fn cancel(mut self) {
    self.shutdown();
  }

  /// Stops the stream and puts the writer thread to rest. Idempotent, because
  /// `Drop` runs it again behind every other path.
  ///
  /// The cancel goes out unconditionally: after a `Stop` the writer has
  /// already returned and the send simply fails, but on every other path it is
  /// what wakes the thread up. Joining without it would wait forever on a
  /// thread blocked reading a channel this very handle still holds open.
  fn shutdown(&mut self) {
    if self.worker.is_none() {
      return;
    }

    for stream in &self.objects.streams {
      stream.stop_with_ch(|_| {});
    }
    self.microphone.take();
    let _ = self.commands.send(Command::Cancel);
    self.join_writer();
  }

  fn join_writer(&mut self) {
    if let Some(worker) = self.worker.take() {
      let _ = worker.join();
    }
  }
}

impl Drop for CaptureSession {
  fn drop(&mut self) {
    // Already done when `stop` or `cancel` ran; this is for the paths that
    // drop the handle outright, such as a start that was cancelled mid-flight.
    self.shutdown();
  }
}

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

#[cfg(test)]
mod tests {
  use std::sync::atomic::AtomicU32;
  use std::time::Duration;

  use super::*;

  /// An NV12 frame of the right shape, standing in for a captured one.
  ///
  /// The picture is drawn rather than left blank on purpose. A fresh
  /// `cv::PixelBuf` is whatever the allocator last had - sometimes zeroes,
  /// sometimes rubbish - and neither tells you anything afterwards: zeroed YUV
  /// decodes to exactly the flat green that a *corrupt* frame region also
  /// decodes to, so a blank source could never distinguish the two. Instead
  /// luma is a top-to-bottom ramp, which makes each band of the image
  /// identifiable by its brightness alone, with a little per-column, per-tick
  /// dither so the encoder has real work to do and a frozen frame shows up.
  /// Chroma is left neutral, so the movie decodes grey and any green is damage.
  fn synthetic_frame(width: u32, height: u32, source_ns: i64, wall: Instant) -> Frame {
    let mut buf = cv::PixelBuf::new(
      width as usize,
      height as usize,
      cv::PixelFormat::_420V,
      None,
    )
    .expect("a pixel buffer");

    let tick = (source_ns / MS) as usize;
    // Locked by hand rather than with `base_address_lock`, whose guard borrows
    // the buffer mutably for as long as it lives and so leaves no way to ask
    // the buffer about its own planes while it is held.
    // SAFETY: the matching unlock is at the end of this block, and nothing in
    // between can return early or panic past it - `plane_*` are pure reads and
    // the writes are bounded by the sizes they report.
    unsafe { buf.lock_base_addr(cv::pixel_buffer::LockFlags::DEFAULT) }
      .result()
      .expect("the pixel buffer locks");
    {
      let (rows, columns) = (buf.plane_height(0), buf.plane_width(0));
      let stride = buf.plane_bytes_per_row(0);
      let luma = buf.plane_base_address(0).cast_mut();
      // A bar that slides down the ramp, so the picture moves without the
      // ramp itself moving - the bands have to keep their brightness for the
      // bottom-of-frame check below to mean anything. Whole rows are filled at
      // a time because a per-pixel loop here is fast enough in a release build
      // and far too slow in the debug build the tests actually run in: it
      // starves the producer thread and shortens the recording.
      let bar = (tick * 6) % rows.max(1);
      for row in 0..rows {
        // SAFETY: the buffer is locked, and the row is within the plane's own
        // height and stride as CoreVideo reports them.
        let line = unsafe { std::slice::from_raw_parts_mut(luma.add(row * stride), columns) };
        line.fill(if row.abs_diff(bar) < 16 {
          235
        } else {
          16 + (200 * row / rows.max(1)) as u8
        });
      }

      let (rows, columns) = (buf.plane_height(1), buf.plane_width(1));
      let stride = buf.plane_bytes_per_row(1);
      let chroma = buf.plane_base_address(1).cast_mut();
      for row in 0..rows {
        // SAFETY: as above; the interleaved plane is two bytes per pixel.
        let line = unsafe { std::slice::from_raw_parts_mut(chroma.add(row * stride), columns * 2) };
        line.fill(128);
      }
    }
    // SAFETY: pairs with the lock above.
    unsafe { buf.unlock_lock_base_addr(cv::pixel_buffer::LockFlags::DEFAULT) }
      .result()
      .expect("the pixel buffer unlocks");

    Frame {
      buf,
      source_ns,
      wall,
    }
  }

  const SOURCE_EPOCH: i64 = 1_000_000_000_000;
  const MS: i64 = 1_000_000;

  const FRAME_MS: u64 = 33;
  static WIDTH: AtomicU32 = AtomicU32::new(640);
  static HEIGHT: AtomicU32 = AtomicU32::new(480);

  #[test]
  fn trims_microphone_preroll_at_whole_pcm_frames() {
    let captured_at = Instant::now();
    let microphone = MicrophoneBuffer {
      captured_at,
      samples: (0..40).map(|sample| sample as f32).collect(),
    };
    let trimmed = microphone_buffer_from_origin(
      microphone,
      captured_at + Duration::from_millis(5),
      MicrophoneFormat {
        channels: 2,
        sample_rate: 1_000,
      },
    )
    .expect("the buffer overlaps time zero");

    assert_eq!(trimmed.captured_at, captured_at + Duration::from_millis(5));
    assert_eq!(
      trimmed.samples,
      (10..40).map(|sample| sample as f32).collect::<Vec<_>>()
    );
  }

  #[test]
  fn drops_a_microphone_buffer_wholly_before_video() {
    let captured_at = Instant::now();
    let microphone = MicrophoneBuffer {
      captured_at,
      samples: vec![0.0; 40],
    };

    assert!(microphone_buffer_from_origin(
      microphone,
      captured_at + Duration::from_millis(30),
      MicrophoneFormat {
        channels: 2,
        sample_rate: 1_000,
      },
    )
    .is_none());
  }

  #[test]
  fn records_into_a_quicktime_movie_that_flushes_a_fragment_every_two_seconds() {
    let container = Container::quicktime_fragmented();
    assert_eq!(container.format.extension(), "mov");
    let interval = container.fragment_interval.expect("a fragment interval");
    assert!((interval.as_secs() - 2.0).abs() < f64::EPSILON);
  }

  /// The container names the file, and the name is what every reader afterwards
  /// believes. These two are written in different modules - one macOS-only, one
  /// not - so nothing but this holds them together.
  #[test]
  fn names_the_working_file_after_the_container_it_is() {
    let name = crate::recording::encoding::temp_file_name(
      chrono::NaiveDate::from_ymd_opt(2026, 8, 8)
        .unwrap()
        .and_hms_milli_opt(14, 32, 5, 250)
        .unwrap(),
    );
    assert!(name.ends_with(&format!(
      ".{}",
      Container::quicktime_fragmented().format.extension()
    )));
  }

  /// What the app records to. Named for what these tests are asking about it,
  /// and taken from production rather than rebuilt, so the evidence below is
  /// always evidence about the shipping container.
  fn fragmented_qt() -> Container {
    Container::quicktime_fragmented()
  }

  /// The default for every test that is asking about timing rather than about
  /// containers: whatever the app itself records to.
  fn record(name: &str, pause: Option<(u64, u64)>, end_at_ms: u64) -> Outcome {
    record_at(
      name,
      pause,
      end_at_ms,
      640,
      480,
      Container::quicktime_fragmented(),
    )
  }

  /// Plays a recording out at real speed on another thread, because the
  /// writer's real-time input paces itself against the wall clock: fed any
  /// faster it reports itself busy and nothing is ever appended.
  fn play(
    commands: mpsc::Sender<Command>,
    base: Instant,
    pause: Option<(u64, u64)>,
    end_at_ms: u64,
  ) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
      let mut at = 0;
      // Which side of the pause the producer is on. A latch rather than a
      // boolean, because a boolean that goes back to "running" at the resume
      // reads as "has not paused yet" on the very next frame and pauses all
      // over again - and a timeline told to resume repeatedly adds the same
      // span to its paused total every time, which drags every later frame
      // back onto the pause instant.
      let (mut sent_pause, mut sent_resume) = (false, false);
      while at < end_at_ms {
        if let Some((pause_at, resume_at)) = pause {
          // Tested by crossing rather than equality: the catch-up below can
          // step over any particular millisecond.
          if !sent_pause && at >= pause_at {
            sent_pause = true;
            let _ = commands.send(Command::Pause {
              at: base + Duration::from_millis(pause_at),
            });
          }
          if sent_pause && !sent_resume && at >= resume_at {
            sent_resume = true;
            let _ = commands.send(Command::Resume {
              at: base + Duration::from_millis(resume_at),
            });
          }
        }
        if commands
          .send(Command::Frame(synthetic_frame(
            WIDTH.load(Ordering::Relaxed),
            HEIGHT.load(Ordering::Relaxed),
            SOURCE_EPOCH + at as i64 * MS,
            Instant::now(),
          )))
          .is_err()
        {
          return;
        }

        // Paced against `base` rather than by sleeping a fixed step, so that
        // however long building a frame takes, this timeline keeps step with
        // the wall clock the writer and the stop signal are both on. A fixed
        // step lets a slow producer fall behind unboundedly and end the
        // recording early, which reads afterwards exactly like the encoder
        // having dropped everything.
        at += FRAME_MS;
        let target = base + Duration::from_millis(at);
        match target.checked_duration_since(Instant::now()) {
          Some(remaining) => std::thread::sleep(remaining),
          // Behind: give up the frames that should already have gone out.
          None => at = at.max(base.elapsed().as_millis() as u64 / FRAME_MS * FRAME_MS),
        }
      }
    })
  }

  struct Outcome {
    appended: u64,
    duration_ms: u64,
    /// Reported because it is the counter that catches the interesting
    /// failure. A wedged encoder never refuses a frame - it is simply never
    /// ready for another one, for the whole rest of the recording - so the
    /// movie comes out truncated while `rejected` stays at zero.
    not_ready: u64,
    rejected: u64,
    result: Result<FinalizeInfo, String>,
  }

  impl Outcome {
    fn report(&self, label: &str) {
      println!(
        "{label}: appended={} not_ready={} rejected={} duration={}ms result={:?}",
        self.appended,
        self.not_ready,
        self.rejected,
        self.duration_ms,
        self.result.as_ref().err()
      );
    }

    /// Asserts the movie is as long as the material fed to it, give or take
    /// the slack of a real-time run. This is what a silent truncation trips.
    fn assert_lasts(&self, expected_ms: u64) {
      assert!(self.result.is_ok(), "{:?}", self.result.as_ref().err());
      assert!(
        self.duration_ms.abs_diff(expected_ms) < 500,
        "the movie is {}ms long, but {expected_ms}ms of frames were fed to it",
        self.duration_ms
      );
    }
  }

  /// Where the encoder tests leave their movies, so `ffprobe` can be pointed
  /// at them afterwards.
  fn test_movie(name: &str, container: Container) -> PathBuf {
    let directory = std::env::temp_dir().join("orbit-capture-tests");
    std::fs::create_dir_all(&directory).unwrap();
    directory.join(format!("{name}.{}", container.format.extension()))
  }

  fn record_at(
    name: &str,
    pause: Option<(u64, u64)>,
    end_at_ms: u64,
    width: u32,
    height: u32,
    container: Container,
  ) -> Outcome {
    let path = test_movie(name, container);
    let _ = std::fs::remove_file(&path);

    WIDTH.store(width, Ordering::Relaxed);
    HEIGHT.store(height, Ordering::Relaxed);
    let stats = Arc::new(CaptureStats::default());
    let writer = Writer::new(WriterConfig {
      path,
      width,
      height,
      fps: 30,
      system_audio: false,
      microphone_format: None,
      stats: Arc::clone(&stats),
      on_failure: Box::new(|reason| println!("failure reported: {reason}")),
      container,
    })
    .expect("a writer");
    let base = writer.base;
    let (commands, inbox) = mpsc::channel();
    let (first_frame, _first_framed) = mpsc::channel();
    let (reply, replies) = mpsc::channel();

    let producer = play(commands.clone(), base, pause, end_at_ms);
    std::thread::spawn(move || {
      std::thread::sleep(Duration::from_millis(end_at_ms + 200));
      let _ = commands.send(Command::Stop {
        at: Instant::now(),
        reply,
      });
    });

    writer.run(&inbox, &first_frame);
    let result = replies.recv().expect("a reply");
    producer.join().unwrap();

    Outcome {
      appended: stats.appended.load(Ordering::Relaxed),
      duration_ms: result.as_ref().map_or(0, |info| info.duration_ms),
      not_ready: stats.not_ready.load(Ordering::Relaxed),
      rejected: stats.rejected.load(Ordering::Relaxed),
      result,
    }
  }

  #[test]
  #[ignore = "drives a real encoder at real speed; run with --ignored"]
  fn writes_a_movie_without_a_pause() {
    let outcome = record("plain", None, 3_000);
    outcome.report("plain");
    outcome.assert_lasts(3_000);
    assert_eq!(outcome.rejected, 0);
  }

  #[test]
  #[ignore = "drives a real encoder at real speed; run with --ignored"]
  fn writes_a_movie_across_a_pause() {
    let outcome = record("paused", Some((990, 3_960)), 6_000);
    outcome.report("paused");
    outcome.assert_lasts(3_030);
    assert_eq!(outcome.rejected, 0, "frames were rejected after the resume");
  }

  /// The real case: a large frame, so the encoder is actually working, and a
  /// long pause, so the movie's clock falls far behind the wall clock.
  #[test]
  #[ignore = "drives a real encoder at real speed; run with --ignored"]
  fn writes_a_large_movie_across_a_long_pause() {
    let outcome = record_at(
      "paused-hd",
      Some((990, 5_940)),
      8_000,
      2560,
      1440,
      Container::mp4(),
    );
    outcome.report("paused-hd");
    outcome.assert_lasts(3_050);
    assert_eq!(outcome.rejected, 0, "frames were rejected after the resume");
  }

  /// A capture the size of a real display, whose height is not a multiple of
  /// the 16-pixel macroblock the encoder works in. An encoder that mishandles
  /// the ragged last row leaves the bottom of the picture as untouched YUV,
  /// which decodes flat green; the assertion below is that it does not.
  #[test]
  #[ignore = "drives a real encoder at real speed; run with --ignored"]
  fn writes_a_movie_at_an_unaligned_display_size() {
    let outcome = record_at(
      "hidpi",
      None,
      3_000,
      3600,
      2338,
      Container::quicktime_fragmented(),
    );
    outcome.report("hidpi");
    outcome.assert_lasts(3_000);
    assert_eq!(outcome.rejected, 0);

    // The average colour of the bottom quarter of every frame. The source ramp
    // makes that band bright grey; the failure being looked for is green,
    // which is what a zeroed - i.e. never written - YUV region decodes to.
    let path = test_movie("hidpi", Container::quicktime_fragmented());
    let decoded = std::process::Command::new("ffmpeg")
      .args(["-v", "error", "-i"])
      .arg(&path)
      .args([
        "-vf",
        "crop=3600:584:0:1754,scale=1:1,format=rgb24",
        "-f",
        "rawvideo",
        "-",
      ])
      .output()
      .expect("ffmpeg is installed");
    assert!(
      decoded.status.success(),
      "the movie did not decode: {}",
      String::from_utf8_lossy(&decoded.stderr)
    );
    assert!(
      decoded.stderr.is_empty(),
      "the decoder complained: {}",
      String::from_utf8_lossy(&decoded.stderr)
    );

    let pixels: Vec<_> = decoded.stdout.chunks_exact(3).collect();
    assert!(!pixels.is_empty(), "no frames came back out");
    println!(
      "hidpi: {} frames, bottom-quarter average rgb {:?}",
      pixels.len(),
      &pixels[..pixels.len().min(6)]
    );
    for (index, pixel) in pixels.iter().enumerate() {
      let (red, green, blue) = (pixel[0], pixel[1], pixel[2]);
      assert!(
        red > 80 && blue > 80,
        "frame {index} has a green bottom quarter: rgb({red}, {green}, {blue})"
      );
    }
  }

  #[test]
  #[ignore = "drives a real encoder at real speed; run with --ignored"]
  fn writes_a_fragmented_movie_without_a_pause() {
    let outcome = record_at("plain-frag", None, 3_000, 640, 480, fragmented_qt());
    outcome.report("plain-frag");
    outcome.assert_lasts(3_000);
    assert_eq!(outcome.rejected, 0);
  }

  #[test]
  #[ignore = "drives a real encoder at real speed; run with --ignored"]
  fn writes_a_fragmented_movie_across_a_pause() {
    let outcome = record_at(
      "paused-frag",
      Some((990, 3_960)),
      6_000,
      640,
      480,
      fragmented_qt(),
    );
    outcome.report("paused-frag");
    outcome.assert_lasts(3_030);
    assert_eq!(outcome.rejected, 0, "frames were rejected after the resume");
  }

  /// The case the unfragmented writer was reported to fail: a large frame and
  /// a pause long enough that several fragment boundaries pass while the
  /// movie's clock stands still.
  #[test]
  #[ignore = "drives a real encoder at real speed; run with --ignored"]
  fn writes_a_large_fragmented_movie_across_a_long_pause() {
    let outcome = record_at(
      "paused-hd-frag",
      Some((990, 5_940)),
      8_000,
      2560,
      1440,
      fragmented_qt(),
    );
    outcome.report("paused-hd-frag");
    outcome.assert_lasts(3_050);
    assert_eq!(outcome.rejected, 0, "frames were rejected after the resume");
  }

  /// The environment variable the crash tests below use to tell a child
  /// process that it is the one meant to die.
  const ABANDON: &str = "ORBIT_ABANDON_RECORDING";

  /// Writes for a while and then kills this process outright, leaving the file
  /// exactly as a crash would. It has to be `abort` rather than an early
  /// return: a test that merely stops calling `finish_writing` still unwinds,
  /// and AVFoundation's own teardown would tidy up the very thing we want to
  /// catch mid-flight.
  fn abandon_a_recording(name: &str, container: Container, write_for_ms: u64) {
    if std::env::var_os(ABANDON).is_none() {
      // Being run as part of the ordinary ignored suite rather than by the
      // crash test. Taking the process down here would end the whole run.
      return;
    }
    let path = test_movie(name, container);
    let _ = std::fs::remove_file(&path);

    WIDTH.store(640, Ordering::Relaxed);
    HEIGHT.store(480, Ordering::Relaxed);
    let stats = Arc::new(CaptureStats::default());
    let writer = Writer::new(WriterConfig {
      path: path.clone(),
      width: 640,
      height: 480,
      fps: 30,
      system_audio: false,
      microphone_format: None,
      stats: Arc::clone(&stats),
      on_failure: Box::new(|reason| println!("failure reported: {reason}")),
      container,
    })
    .expect("a writer");
    let base = writer.base;
    let (commands, inbox) = mpsc::channel();
    let (first_frame, _first_framed) = mpsc::channel();

    // Kept feeding past the kill point so the writer is genuinely mid-recording
    // when the process dies, not idling at the end of its material.
    let _producer = play(commands, base, None, write_for_ms * 2);
    std::thread::spawn(move || writer.run(&inbox, &first_frame));

    std::thread::sleep(Duration::from_millis(write_for_ms));
    println!(
      "abandoning after {write_for_ms}ms with {} appended: {}",
      stats.appended.load(Ordering::Relaxed),
      path.display()
    );
    std::io::Write::flush(&mut std::io::stdout()).ok();
    std::process::abort();
  }

  /// Re-runs this very test binary for one of the abandon helpers above, so
  /// the abort lands in a process of its own.
  fn crash(test: &str) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().expect("this test binary"))
      .args([
        test,
        "--ignored",
        "--exact",
        "--nocapture",
        "--test-threads=1",
      ])
      .env(ABANDON, "1")
      .output()
      .expect("the child test binary ran")
  }

  fn ffprobe(path: &std::path::Path) -> std::process::Output {
    std::process::Command::new("ffprobe")
      .args([
        "-hide_banner",
        "-show_format",
        "-show_streams",
        "-of",
        "flat",
      ])
      .arg(path)
      .output()
      .expect("ffprobe is installed")
  }

  fn report_abandoned(label: &str, path: &std::path::Path) {
    let size = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    let probe = ffprobe(path);
    println!("--- {label}: {} bytes at {}", size, path.display());
    println!("{label}: ffprobe status={}", probe.status);
    println!("{}", String::from_utf8_lossy(&probe.stdout));
    println!("{}", String::from_utf8_lossy(&probe.stderr));
  }

  #[test]
  #[ignore = "kills a child process on purpose; run with --ignored"]
  fn writes_a_fragmented_movie_then_dies() {
    abandon_a_recording("abandoned-frag", fragmented_qt(), 10_000);
  }

  #[test]
  #[ignore = "kills a child process on purpose; run with --ignored"]
  fn writes_a_plain_movie_then_dies() {
    abandon_a_recording("abandoned-plain", Container::mp4(), 10_000);
  }

  /// What the whole fragment question is actually about: after an unclean
  /// death, is there a recording left or not?
  #[test]
  #[ignore = "spawns and kills child processes; run with --ignored"]
  fn a_fragmented_movie_survives_a_crash_where_a_plain_one_does_not() {
    let fragmented = crash("recording::platform::tests::writes_a_fragmented_movie_then_dies");
    println!("{}", String::from_utf8_lossy(&fragmented.stdout));
    let plain = crash("recording::platform::tests::writes_a_plain_movie_then_dies");
    println!("{}", String::from_utf8_lossy(&plain.stdout));

    let frag_path = test_movie("abandoned-frag", fragmented_qt());
    let plain_path = test_movie("abandoned-plain", Container::mp4());
    report_abandoned("fragmented .mov", &frag_path);
    report_abandoned("plain .mp4", &plain_path);

    assert!(
      ffprobe(&frag_path).status.success(),
      "the abandoned fragmented movie should still be readable"
    );
  }
}
