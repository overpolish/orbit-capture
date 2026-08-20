// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// Playback lifecycle extension point for future timeline range policies.
impl PreviewPlayerManager {
  fn cancel_worker(&mut self) {
    if let Some(worker) = self.worker.take() {
      self.position_ms = worker.cancel();
    }
  }

  /// Takes the worker without joining it, so the caller can join it off the
  /// main thread. Signalling here still happens under the state lock: the
  /// dying worker has to stop presenting frames, and its displayed position
  /// has to be recorded, before whatever replaces it starts decoding. Any
  /// `cancel_worker` the caller reaches afterwards - `restart`'s, for
  /// instance - is then a no-op.
  pub(super) fn take_worker(&mut self) -> Option<PreviewPlayerWorker> {
    let worker = self.worker.take()?;
    worker.signal_cancel();
    self.position_ms = worker.position();
    Some(worker)
  }

  pub(super) fn restart(&mut self, mode: PlaybackMode) -> Result<(), String> {
    self.cancel_worker();
    let sources = self
      .sources
      .clone()
      .ok_or_else(|| "The recording preview player is not open".to_owned())?;
    sources
      .playing
      .store(matches!(mode, PlaybackMode::Playing), Ordering::Release);
    if matches!(mode, PlaybackMode::Still) {
      if let Some(surface) = &sources.preview_surface {
        surface.hide();
      }
    }
    let event_channel = self
      .event_channel
      .clone()
      .ok_or_else(|| "The recording preview event channel is unavailable".to_owned())?;
    if platform::NATIVE_STILLS
      && matches!(mode, PlaybackMode::Still | PlaybackMode::InteractiveStill)
      && !sources.layout.panes.is_empty()
    {
      let rough = std::mem::take(&mut self.rough_seek);
      if self
        .still_decoder
        .as_ref()
        .is_some_and(platform::StillDecoder::is_finished)
      {
        // A decoder thread can terminate after a native composition failure.
        // Do not retain its disconnected sender: the next scrub/settings
        // update should recreate the decoder instead of reporting a stale
        // "decoder stopped" error forever.
        if let Some(decoder) = self.still_decoder.take() {
          decoder.stop();
        }
      }
      if self.still_decoder.is_none() {
        self.still_decoder = Some(platform::StillDecoder::spawn(sources, event_channel)?);
      }
      return self
        .still_decoder
        .as_ref()
        .ok_or_else(|| "The native preview decoder is unavailable".to_owned())?
        .seek(
          self.position_ms,
          self.latest_seek_request,
          rough,
          self.pane_target_sizes.clone(),
        );
    }
    self.rough_seek = false;
    let playback_factors = self.playback_factors(&sources);
    self.worker = Some(PreviewPlayerWorker::spawn(
      sources,
      worker::WorkerLaunch {
        audio: PreviewAudioSettings {
          audio_track_volumes: self.audio_volumes.clone(),
          enabled_stream_indices: self.audio_indices.clone(),
        },
        mode,
        playback_factors,
        request_id: self.latest_seek_request,
        start_ms: self.position_ms,
      },
      event_channel,
    )?);
    Ok(())
  }

  /// How much each pane's playback decode shrinks to match the on-screen pane
  /// size, mirroring what the still decoder presents.
  fn playback_factors(&self, sources: &PlayerSources) -> Vec<f64> {
    platform::playback_factors(&self.pane_target_sizes, sources)
  }

  pub(super) fn stop(&mut self) {
    self.cancel_worker();
    if let Some(sources) = self.sources.as_ref() {
      sources.playing.store(false, Ordering::Release);
      if let Some(surface) = sources.preview_surface.as_ref() {
        surface.hide();
      }
    }
    if let Some(decoder) = self.still_decoder.take() {
      decoder.stop();
    }
    self.artifact_id = None;
    self.event_channel = None;
    self.is_playing = false;
    self.pane_target_sizes.clear();
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
      self.selection_gesture = None;
    }
    self.sources = None;
    self.session_id = None;
    self.workspace_scene = None;
  }

  pub(super) fn require_session(&self, session_id: u64) -> Result<(), String> {
    (self.session_id == Some(session_id))
      .then_some(())
      .ok_or_else(|| "That recording preview player session is no longer active".to_owned())
  }
}
