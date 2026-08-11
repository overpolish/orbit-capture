// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which recorded audio tracks a derived file carries, and how they are laid
//! out in it.
//!
//! The export path keeps every included track as its own track
//! ([`AudioLayout::SeparateTracks`]) so system audio and a voice-over can still
//! be balanced, soloed or muted afterwards. Collapsing them into one is an
//! explicit export option.

use super::{AudioTrackVolume, RecordingAudioTrack};

/// The bitrate the mixdown is encoded at. Summing tracks means decoding them,
/// so this is the one place in the app that re-encodes audio; generous enough
/// that the mix is not what a person hears a problem in.
const MIXDOWN_BITRATE_BPS: u64 = 192_000;
const MIXDOWN_BITRATE: &str = "192k";

/// How the selected tracks appear in the file being produced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioLayout {
  /// Every selected track summed into a single encoded track when the user
  /// enables “Collapse audio tracks”.
  Mixdown,
  /// Every selected track kept as its own stream-copied track. What an export
  /// writes unless the user asks for the tracks to be collapsed.
  ///
  /// Written and tested ahead of the export path that will use it, hence the
  /// allowance: the point of it existing now is that the mixdown below cannot
  /// quietly become the definition of what saving a recording does.
  #[allow(dead_code)]
  SeparateTracks,
}

/// The recorded audio tracks a derived file should carry, in recording order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TrackSelection {
  stream_indices: Vec<usize>,
  volumes: Vec<(usize, i16)>,
}

impl TrackSelection {
  /// Reads a selection from what the window's toggle rows are set to.
  ///
  /// Indices that name no track in this recording are dropped, and the rest
  /// are put back into the recording's own order: the window sends the rows it
  /// is showing, and a stale or reordered row must not become a mapping FFmpeg
  /// would refuse or, worse, one that quietly maps the wrong track.
  pub fn new(tracks: &[RecordingAudioTrack], enabled: &[usize]) -> Self {
    Self::with_volumes(tracks, enabled, &[]).expect("zero-decibel track volumes are valid")
  }

  pub fn with_volumes(
    tracks: &[RecordingAudioTrack],
    enabled: &[usize],
    volumes: &[AudioTrackVolume],
  ) -> Result<Self, String> {
    if volumes
      .iter()
      .any(|volume| !(-60..=12).contains(&volume.decibels))
    {
      return Err("Audio volume must be between -60 dB and +12 dB".to_owned());
    }
    let stream_indices = tracks
      .iter()
      .map(|track| track.stream_index)
      .filter(|index| enabled.contains(index))
      .collect::<Vec<_>>();
    Ok(Self {
      volumes: volumes
        .iter()
        .filter(|volume| stream_indices.contains(&volume.stream_index) && volume.decibels != 0)
        .map(|volume| (volume.stream_index, volume.decibels))
        .collect(),
      stream_indices,
    })
  }

  fn volume(&self, stream_index: usize) -> i16 {
    self
      .volumes
      .iter()
      .find_map(|(index, decibels)| (*index == stream_index).then_some(*decibels))
      .unwrap_or(0)
  }

  /// Whether this selection leaves nothing out.
  pub fn covers(&self, tracks: &[RecordingAudioTrack]) -> bool {
    self.stream_indices.len() == tracks.len()
  }

  /// Whether this choice needs more than the ordinary all-stream remux.
  ///
  /// Leaving a track out always needs explicit mapping. A mixdown only needs
  /// processing when there is actually more than one input to sum; asking to
  /// collapse a lone track must not re-encode it for no audible difference.
  pub fn needs_processing(&self, tracks: &[RecordingAudioTrack], layout: AudioLayout) -> bool {
    !self.covers(tracks)
      || !self.volumes.is_empty()
      || matches!(layout, AudioLayout::Mixdown) && self.stream_indices.len() > 1
  }

  /// The selected audio's expected encoded size.
  ///
  /// Recorded AAC is copied, so its configured bitrate is the useful estimate.
  /// A recovered track has no kind metadata; treating it like the larger
  /// system-audio stream is safer than promising a file that is too small.
  pub fn estimated_audio_bytes(
    &self,
    tracks: &[RecordingAudioTrack],
    layout: AudioLayout,
    duration_ms: u64,
  ) -> u64 {
    if self.stream_indices.is_empty() {
      return 0;
    }

    let bitrate = if (matches!(layout, AudioLayout::Mixdown) && self.stream_indices.len() > 1)
      || !self.volumes.is_empty()
    {
      if matches!(layout, AudioLayout::SeparateTracks) {
        MIXDOWN_BITRATE_BPS.saturating_mul(self.stream_indices.len() as u64)
      } else {
        MIXDOWN_BITRATE_BPS
      }
    } else {
      tracks
        .iter()
        .filter(|track| self.stream_indices.contains(&track.stream_index))
        .map(|track| match track.kind {
          super::AudioTrackKind::Microphone => 128_000,
          super::AudioTrackKind::SystemAudio | super::AudioTrackKind::Unknown => 192_000,
        })
        .sum()
    };

    bitrate.saturating_mul(duration_ms) / 8_000
  }

  /// The FFmpeg arguments that put this selection into the output.
  ///
  /// Video is never among them: nothing here touches the picture. Callers pair
  /// these with either a stream copy or the requested compression encode.
  pub fn audio_args(&self, layout: AudioLayout) -> Vec<String> {
    self.audio_args_from(layout, 0)
  }

  /// The same mapping when video and recorded audio are separate FFmpeg
  /// inputs, such as camera export processing.
  pub fn audio_args_from(&self, layout: AudioLayout, input: usize) -> Vec<String> {
    if self.stream_indices.is_empty() {
      return vec!["-an".to_owned()];
    }

    let has_volume_changes = !self.volumes.is_empty();
    match layout {
      // One track needs no summing, so it crosses untouched rather than being
      // decoded and re-encoded for the sake of passing through a filter.
      AudioLayout::Mixdown if self.stream_indices.len() == 1 && !has_volume_changes => vec![
        "-map".to_owned(),
        format!("{input}:a:{}", self.stream_indices[0]),
        "-c:a".to_owned(),
        "copy".to_owned(),
      ],
      AudioLayout::Mixdown if !has_volume_changes => {
        let inputs: String = self
          .stream_indices
          .iter()
          .map(|index| format!("[{input}:a:{index}]"))
          .collect();
        vec![
          "-filter_complex".to_owned(),
          format!(
            "{inputs}amix=inputs={}:normalize=0[mix]",
            self.stream_indices.len()
          ),
          "-map".to_owned(),
          "[mix]".to_owned(),
          "-c:a".to_owned(),
          "aac".to_owned(),
          "-b:a".to_owned(),
          MIXDOWN_BITRATE.to_owned(),
        ]
      }
      AudioLayout::Mixdown => {
        let mut filters = String::new();
        let mut inputs = String::new();
        for (position, index) in self.stream_indices.iter().enumerate() {
          filters.push_str(&format!(
            "[{input}:a:{index}]volume={}dB[track{position}];",
            self.volume(*index)
          ));
          inputs.push_str(&format!("[track{position}]"));
        }

        vec![
          "-filter_complex".to_owned(),
          // `normalize=0` is the whole point: amix divides by the number of
          // inputs by default, so including a second track would make the
          // first one quieter than it was recorded. A person toggling
          // microphone on must not hear the system audio drop by half.
          format!(
            "{filters}{inputs}amix=inputs={}:normalize=0[mix]",
            self.stream_indices.len()
          ),
          "-map".to_owned(),
          "[mix]".to_owned(),
          "-c:a".to_owned(),
          "aac".to_owned(),
          "-b:a".to_owned(),
          MIXDOWN_BITRATE.to_owned(),
        ]
      }
      AudioLayout::SeparateTracks if !has_volume_changes => {
        let mut args = Vec::with_capacity(self.stream_indices.len() * 2 + 2);
        for index in &self.stream_indices {
          args.push("-map".to_owned());
          args.push(format!("{input}:a:{index}"));
        }
        args.push("-c:a".to_owned());
        args.push("copy".to_owned());

        args
      }
      AudioLayout::SeparateTracks => {
        let mut filters = String::new();
        let mut args = Vec::new();
        for (position, index) in self.stream_indices.iter().enumerate() {
          filters.push_str(&format!(
            "[{input}:a:{index}]volume={}dB[track{position}];",
            self.volume(*index)
          ));
          args.extend(["-map".to_owned(), format!("[track{position}]")]);
        }
        filters.pop();
        args.splice(0..0, ["-filter_complex".to_owned(), filters]);
        args.extend([
          "-c:a".to_owned(),
          "aac".to_owned(),
          "-b:a".to_owned(),
          MIXDOWN_BITRATE.to_owned(),
        ]);
        args
      }
    }
  }
}

#[cfg(test)]
#[path = "track_selection_tests.rs"]
mod tests;
