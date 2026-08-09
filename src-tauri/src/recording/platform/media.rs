// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::{Duration, Instant};

use cidre::{arc, av, cat, cf, cm, ns, os, sc};

use super::{
  AudioSample, MicrophoneBuffer, MicrophoneFormat, MICROPHONE_AUDIO_BITRATE, NANOS_PER_SEC,
  SYSTEM_AUDIO_BITRATE, SYSTEM_AUDIO_CHANNELS, SYSTEM_AUDIO_SAMPLE_RATE,
};
use crate::recording::encoding::bitrate_bps;

/// Reads the frame status ScreenCaptureKit attaches to every sample.
///
/// The attachment dictionary is keyed by `cf::String` while the constant is an
/// `ns::String`, which is why the key is bridged rather than used directly.
pub(super) fn frame_status(sample: &cm::SampleBuf) -> Option<sc::FrameStatus> {
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
pub(super) fn time_to_ns(time: cm::Time) -> Option<i64> {
  if time.scale <= 0 || !time.flags.contains(cm::TimeFlags::VALID) {
    return None;
  }
  let nanos = i128::from(time.value) * i128::from(NANOS_PER_SEC) / i128::from(time.scale);

  i64::try_from(nanos).ok()
}

pub(super) fn nanos(time: i64) -> cm::Time {
  cm::Time::new(time, NANOS_PER_SEC as cm::TimeScale)
}

/// H.264 subsamples chroma, so an odd edge has nowhere to put half a pixel.
/// The stream and the encoder are both configured with the rounded size so
/// they can never disagree about what a frame is.
pub(super) const fn even(value: u32) -> u32 {
  value & !1
}

/// The encoder settings for one recording.
pub(super) fn video_settings(
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

pub(super) fn system_audio_settings() -> arc::R<ns::DictionaryMut<ns::String, ns::Id>> {
  audio_settings(
    SYSTEM_AUDIO_SAMPLE_RATE,
    SYSTEM_AUDIO_CHANNELS,
    SYSTEM_AUDIO_BITRATE,
  )
}

pub(super) fn microphone_audio_settings(
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
pub(super) fn audio_sample_from_origin(
  sample: AudioSample,
  origin_source_ns: i64,
) -> Option<AudioSample> {
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

pub(super) fn audio_sample_with_pts(
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

pub(super) fn microphone_format_description(
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

pub(super) fn microphone_sample_buffer(
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

pub(super) fn microphone_buffer_from_origin(
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
