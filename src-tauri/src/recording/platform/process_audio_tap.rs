// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use cidre::core_audio::aggregate_device_keys;
use cidre::core_audio::hardware::{sub_tap_keys, StartedDevice};

struct TapCallback {
  commands: SyncSender<Command>,
  stats: Arc<CaptureStats>,
}

pub(super) struct ProcessAudioTap {
  _device: StartedDevice<core_audio::AggregateDevice>,
  _tap: core_audio::TapGuard,
  _callback: Box<TapCallback>,
}

impl ProcessAudioTap {
  pub(super) fn start(
    selection: &crate::recording::SystemAudioSelection,
    commands: SyncSender<Command>,
    stats: Arc<CaptureStats>,
  ) -> Result<Self, String> {
    let process_ids = process_object_ids(&selection.process_ids);
    let description = if selection.application_ids.is_empty() {
      let current = core_audio::Process::with_pid(std::process::id() as i32)
        .map_err(|error| error.to_string())?;
      let excluded: arc::R<ns::Array<ns::Number>> = (&[current.0 .0][..]).into();
      core_audio::TapDesc::with_stereo_global_tap_excluding_processes(&excluded)
    } else {
      if process_ids.is_empty() {
        return Err("None of the selected apps expose an audio process".to_owned());
      }
      let included: arc::R<ns::Array<ns::Number>> = process_ids.as_slice().into();
      core_audio::TapDesc::with_stereo_mixdown_of_processes(&included)
    };
    let tap = description
      .create_process_tap()
      .map_err(|error| error.to_string())?;
    validate_format(tap.asbd().map_err(|error| error.to_string())?)?;

    let tap_uid = tap.uid().map_err(|error| error.to_string())?;
    let tap_entry = cf::DictionaryOf::<cf::String, cf::Type>::with_keys_values(
      &[sub_tap_keys::uid()],
      &[tap_uid.as_type_ref()],
    );
    let tap_list = cf::ArrayOf::from_slice(&[tap_entry.as_ref()]);
    let aggregate_uid = cf::Uuid::new().to_cf_string();
    let aggregate = cf::DictionaryOf::<cf::String, cf::Type>::with_keys_values(
      &[
        aggregate_device_keys::is_private(),
        aggregate_device_keys::is_stacked(),
        aggregate_device_keys::tap_auto_start(),
        aggregate_device_keys::name(),
        aggregate_device_keys::uid(),
        aggregate_device_keys::tap_list(),
      ],
      &[
        cf::Boolean::value_true().as_type_ref(),
        cf::Boolean::value_false(),
        cf::Boolean::value_true(),
        cf::str!(c"Orbit Capture System Audio").as_type_ref(),
        aggregate_uid.as_type_ref(),
        tap_list.as_type_ref(),
      ],
    );
    let device =
      core_audio::AggregateDevice::with_desc(&aggregate).map_err(|error| error.to_string())?;
    let mut callback = Box::new(TapCallback { commands, stats });
    let proc_id = device
      .create_io_proc_id(process_audio_callback, Some(callback.as_mut()))
      .map_err(|error| error.to_string())?;
    let device =
      core_audio::device_start(device, Some(proc_id)).map_err(|error| error.to_string())?;

    Ok(Self {
      _device: device,
      _tap: tap,
      _callback: callback,
    })
  }
}

fn process_object_ids(process_ids: &[u32]) -> Vec<u32> {
  process_ids
    .iter()
    .filter_map(|process_id| {
      core_audio::Process::with_pid(*process_id as i32)
        .ok()
        .map(|process| process.0 .0)
    })
    .collect()
}

fn validate_format(format: cat::AudioStreamBasicDesc) -> Result<(), String> {
  let is_float = format.format == cat::AudioFormat::LINEAR_PCM
    && format
      .format_flags
      .contains(cat::audio::FormatFlags::IS_FLOAT)
    && format.bits_per_channel == 32;
  if !is_float
    || format.channels_per_frame != SYSTEM_AUDIO_CHANNELS as u32
    || format.sample_rate.round() as i64 != SYSTEM_AUDIO_SAMPLE_RATE
  {
    return Err(format!(
      "The system-audio tap delivered an unsupported format: {format:?}"
    ));
  }
  Ok(())
}

extern "C" fn process_audio_callback(
  _device: core_audio::Device,
  _now: &cat::AudioTimeStamp,
  input: &cat::AudioBufList<2>,
  _input_time: &cat::AudioTimeStamp,
  _output: &mut cat::AudioBufList<1>,
  _output_time: &cat::AudioTimeStamp,
  callback: Option<&mut TapCallback>,
) -> os::Status {
  let Some(callback) = callback else {
    return os::Status::NO_ERR;
  };
  let Some(samples) = copy_stereo_samples(input) else {
    return os::Status::NO_ERR;
  };
  let sample = SystemAudioSample::Pcm(MicrophoneBuffer {
    captured_at: Instant::now(),
    samples,
  });
  if let Err(TrySendError::Full(_)) = callback.commands.try_send(Command::SystemAudio(sample)) {
    callback.stats.audio_dropped.fetch_add(1, Ordering::Relaxed);
  }
  os::Status::NO_ERR
}

fn copy_stereo_samples(input: &cat::AudioBufList<2>) -> Option<Vec<f32>> {
  let buffers = &input.buffers[..usize::try_from(input.number_buffers).ok()?.min(2)];
  if buffers.len() == 1 && buffers[0].number_channels == 2 {
    return Some(unsafe { float_samples(&buffers[0]) }.to_vec());
  }
  if buffers.len() != 2 {
    return None;
  }
  let left = unsafe { float_samples(&buffers[0]) };
  let right = unsafe { float_samples(&buffers[1]) };
  let frames = left.len().min(right.len());
  let mut samples = Vec::with_capacity(frames * 2);
  for index in 0..frames {
    samples.extend_from_slice(&[left[index], right[index]]);
  }
  Some(samples)
}

unsafe fn float_samples(buffer: &cat::AudioBuf) -> &[f32] {
  if buffer.data.is_null() {
    return &[];
  }
  unsafe {
    std::slice::from_raw_parts(
      buffer.data.cast::<f32>(),
      buffer.data_bytes_size as usize / size_of::<f32>(),
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn audio_buffer(samples: &mut [f32], channels: u32) -> cat::AudioBuf {
    cat::AudioBuf {
      number_channels: channels,
      data_bytes_size: size_of_val(samples) as u32,
      data: samples.as_mut_ptr().cast(),
    }
  }

  #[test]
  fn keeps_an_interleaved_stereo_tap_interleaved() {
    let mut samples = [0.1, 0.2, 0.3, 0.4];
    let input = cat::AudioBufList {
      number_buffers: 1,
      buffers: [
        audio_buffer(&mut samples, 2),
        cat::AudioBuf {
          number_channels: 0,
          data_bytes_size: 0,
          data: std::ptr::null_mut(),
        },
      ],
    };

    assert_eq!(copy_stereo_samples(&input), Some(samples.to_vec()));
  }

  #[test]
  fn interleaves_a_planar_stereo_tap() {
    let mut left = [0.1, 0.3];
    let mut right = [0.2, 0.4];
    let input = cat::AudioBufList {
      number_buffers: 2,
      buffers: [audio_buffer(&mut left, 1), audio_buffer(&mut right, 1)],
    };

    assert_eq!(copy_stereo_samples(&input), Some(vec![0.1, 0.2, 0.3, 0.4]));
  }
}
