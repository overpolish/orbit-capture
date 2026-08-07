#![allow(clippy::useless_transmute)]

use std::collections::HashSet;

use cidre::{
  arc, cm, define_obj_type, dispatch, ns, objc, sc,
  sc::stream::{Output, OutputImpl},
};
use tauri::ipc::Channel;

use super::{AudioPreviewEvent, LevelAccumulator};

#[repr(C)]
struct AudioPreviewOutputInner {
  channel: Channel<AudioPreviewEvent>,
  level: LevelAccumulator,
}

impl AudioPreviewOutputInner {
  fn handle_audio(&mut self, sample_buffer: &cm::SampleBuf) {
    let Some(format) = sample_buffer.format_desc() else {
      return;
    };
    let Some(description) = format.stream_basic_desc() else {
      return;
    };
    if !description.is_common_f32() {
      return;
    }
    let Ok(buffers) = sample_buffer.audio_buf_list::<2>() else {
      return;
    };
    let list = buffers.list();
    let samples = list.buffers[..list.number_buffers as usize]
      .iter()
      .flat_map(|buffer| unsafe {
        std::slice::from_raw_parts(
          buffer.data.cast::<f32>(),
          buffer.data_bytes_size as usize / size_of::<f32>(),
        )
      })
      .copied()
      .map(f64::from);

    self.level.push(samples, |decibels| {
      let _ = self.channel.send(AudioPreviewEvent::Signal { decibels });
    });
  }
}

define_obj_type!(
  AudioPreviewOutput + OutputImpl,
  AudioPreviewOutputInner,
  AUDIO_PREVIEW_OUTPUT_CLS
);

impl Output for AudioPreviewOutput {}

#[objc::add_methods]
impl OutputImpl for AudioPreviewOutput {
  extern "C" fn impl_stream_did_output_sample_buf(
    &mut self,
    _command: Option<&objc::Sel>,
    _stream: &sc::Stream,
    sample_buffer: &mut cm::SampleBuf,
    kind: sc::OutputType,
  ) {
    if kind == sc::OutputType::Audio {
      self.inner_mut().handle_audio(sample_buffer);
    }
  }
}

pub struct FilteredAudioPreview {
  _output: arc::R<AudioPreviewOutput>,
  _queue: arc::R<dispatch::Queue>,
  stream: arc::R<sc::Stream>,
}

impl Drop for FilteredAudioPreview {
  fn drop(&mut self) {
    self.stream.stop_with_ch(|_| {});
  }
}

pub async fn start_filtered_audio_preview(
  application_ids: Vec<String>,
  channel: Channel<AudioPreviewEvent>,
) -> Result<FilteredAudioPreview, String> {
  let selected = application_ids.into_iter().collect::<HashSet<_>>();
  let content = sc::ShareableContent::current()
    .await
    .map_err(|error| error.to_string())?;
  let displays = content.displays();
  let display = displays
    .first()
    .ok_or_else(|| "No display is available for application audio capture".to_owned())?;
  let applications = content
    .apps()
    .iter()
    .filter(|application| selected.contains(&application.bundle_id().to_string()))
    .map(|application| application.retained())
    .collect::<Vec<_>>();
  if applications.is_empty() {
    return Err("None of the selected applications are currently available".into());
  }

  let applications = ns::Array::from_slice_retained(&applications);
  let excluded_windows = ns::Array::new();
  let filter = sc::ContentFilter::with_display_including_apps_excepting_windows(
    display,
    &applications,
    &excluded_windows,
  );
  let mut configuration = sc::StreamCfg::new();
  configuration.set_captures_audio(true);
  configuration.set_excludes_current_process_audio(true);
  configuration.set_sample_rate(48_000);
  configuration.set_channel_count(2);

  let output = AudioPreviewOutput::with(AudioPreviewOutputInner {
    channel,
    level: LevelAccumulator::with_format(48_000, 2),
  });
  let queue = dispatch::Queue::serial_with_ar_pool();
  let stream = sc::Stream::new(&filter, &configuration);
  stream
    .add_stream_output(output.as_ref(), sc::OutputType::Audio, Some(&queue))
    .map_err(|error| error.to_string())?;
  stream.start().await.map_err(|error| error.to_string())?;

  Ok(FilteredAudioPreview {
    _output: output,
    _queue: queue,
    stream,
  })
}
