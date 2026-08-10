// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

impl AudioWriter {
  fn finish(&mut self, at: Instant) -> Result<FinalizeInfo, String> {
    if !self.timeline.has_started() {
      self.writer.cancel_writing();
      return Err("The audio recording did not start".to_owned());
    }
    let stop_ns = self.timeline.stop_pts_ns(self.elapsed_ns(at)).max(1);
    self.pad_tracks_to(stop_ns);
    if let Some(input) = self.system_audio_input.as_mut() {
      input.mark_as_finished();
    }
    if let Some(input) = self.microphone_input.as_mut() {
      input.mark_as_finished();
    }
    let end_ns = self
      .system_audio_end_ns
      .unwrap_or(0)
      .max(self.microphone_end_ns.unwrap_or(0));
    self
      .writer
      .end_session_at_src_time(nanos(end_ns))
      .map_err(|error| error.to_string())?;
    self.writer.finish_writing();
    if self.writer.status() != av::AssetWriterStatus::Completed {
      return Err(asset_writer_error(
        &self.writer,
        "The audio recording could not be saved",
      ));
    }
    if let Some(error) = self.failed.take() {
      return Err(error);
    }
    Ok(FinalizeInfo {
      camera: None,
      has_microphone: self.microphone_input.is_some(),
      has_system_audio: self.system_audio_input.is_some(),
      duration_ms: u64::try_from(end_ns / NANOS_PER_MS).unwrap_or_default(),
      height: 0,
      path: self.path.clone(),
      poster: None,
      primary_kind: PrimaryRecordingKind::Audio,
      source_scale_factor: 1.0,
      width: 0,
    })
  }

  pub(in crate::recording::platform) fn run(
    mut self,
    inbox: &Receiver<Command>,
    ready: mpsc::Sender<Result<(), String>>,
  ) {
    while let Ok(command) = inbox.recv() {
      match command {
        Command::Begin { at } => {
          let _ = ready.send(self.begin(at));
        }
        Command::SystemAudio(sample) if self.origin.is_none() => {
          if self.pending_system_audio.len() == SYSTEM_AUDIO_PREROLL_LIMIT {
            self.pending_system_audio.pop_front();
          }
          self.pending_system_audio.push_back(sample);
        }
        Command::SystemAudio(sample) => self.append_system_audio(sample),
        Command::Microphone(buffer) => {
          if self.origin.is_none() {
            if self.pending_microphone.len() == MICROPHONE_PREROLL_LIMIT {
              self.pending_microphone.pop_front();
            }
            self.pending_microphone.push_back(buffer);
          } else if let Some(format) = self.microphone_format {
            self.append_pcm(buffer, format, false);
          }
        }
        Command::MicrophoneFailed(error) => self.fail(error),
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
        Command::Frame(_) => self.fail("Audio-only capture received a video frame".to_owned()),
      }
    }
    self.writer.cancel_writing();
  }
}
