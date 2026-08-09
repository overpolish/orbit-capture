// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which recorded audio tracks a derived file carries, and how they are laid
//! out in it.
//!
//! # The preview mix is a playback mechanism, not export semantics
//!
//! A `<video>` element plays exactly one audio track. Hearing two recorded
//! tracks at once therefore requires summing them into one before playback,
//! which is what [`AudioLayout::Mixdown`] exists for. That is a limitation of
//! the player and nothing else: it says nothing about what saving the
//! recording should produce.
//!
//! The export path keeps every included track as its own track
//! ([`AudioLayout::SeparateTracks`]) so system audio and a voice-over can still
//! be balanced, soloed or muted afterwards. Collapsing them into one is an
//! opt-in the user asks for, not a consequence of having previewed them.
//!
//! Both paths share this type so the toggle rows mean the same thing in each,
//! and only the layout differs. Do not reach for the mixdown because the
//! preview uses it.

use super::RecordingAudioTrack;

/// The bitrate the mixdown is encoded at. Summing tracks means decoding them,
/// so this is the one place in the app that re-encodes audio; generous enough
/// that the mix is not what a person hears a problem in.
const MIXDOWN_BITRATE_BPS: u64 = 192_000;
const MIXDOWN_BITRATE: &str = "192k";

/// How the selected tracks appear in the file being produced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioLayout {
  /// Every selected track summed into a single encoded track. The only layout
  /// a browser video element can play in full, and so the preview's.
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
}

impl TrackSelection {
  /// Reads a selection from what the window's toggle rows are set to.
  ///
  /// Indices that name no track in this recording are dropped, and the rest
  /// are put back into the recording's own order: the window sends the rows it
  /// is showing, and a stale or reordered row must not become a mapping FFmpeg
  /// would refuse or, worse, one that quietly maps the wrong track.
  pub fn new(tracks: &[RecordingAudioTrack], enabled: &[usize]) -> Self {
    Self {
      stream_indices: tracks
        .iter()
        .map(|track| track.stream_index)
        .filter(|index| enabled.contains(index))
        .collect(),
    }
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
    !self.covers(tracks) || matches!(layout, AudioLayout::Mixdown) && self.stream_indices.len() > 1
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

    let bitrate = if matches!(layout, AudioLayout::Mixdown) && self.stream_indices.len() > 1 {
      MIXDOWN_BITRATE_BPS
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

  /// A stable, file-name-safe name for this combination of tracks.
  ///
  /// Derived files are named with it, which is what makes flipping a toggle
  /// back instant: the file for that combination is already on disk.
  pub fn signature(&self) -> String {
    if self.stream_indices.is_empty() {
      return "silent".to_owned();
    }

    self
      .stream_indices
      .iter()
      .map(usize::to_string)
      .collect::<Vec<_>>()
      .join("-")
  }

  /// The FFmpeg arguments that put this selection into the output.
  ///
  /// Video is never among them: nothing here touches the picture. Callers pair
  /// these with either a stream copy or the requested compression encode.
  pub fn audio_args(&self, layout: AudioLayout) -> Vec<String> {
    if self.stream_indices.is_empty() {
      return vec!["-an".to_owned()];
    }

    match layout {
      // One track needs no summing, so it crosses untouched rather than being
      // decoded and re-encoded for the sake of passing through a filter.
      AudioLayout::Mixdown if self.stream_indices.len() == 1 => vec![
        "-map".to_owned(),
        format!("0:a:{}", self.stream_indices[0]),
        "-c:a".to_owned(),
        "copy".to_owned(),
      ],
      AudioLayout::Mixdown => {
        let inputs: String = self
          .stream_indices
          .iter()
          .map(|index| format!("[0:a:{index}]"))
          .collect();

        vec![
          "-filter_complex".to_owned(),
          // `normalize=0` is the whole point: amix divides by the number of
          // inputs by default, so including a second track would make the
          // first one quieter than it was recorded. A person toggling
          // microphone on must not hear the system audio drop by half.
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
      AudioLayout::SeparateTracks => {
        let mut args = Vec::with_capacity(self.stream_indices.len() * 2 + 2);
        for index in &self.stream_indices {
          args.push("-map".to_owned());
          args.push(format!("0:a:{index}"));
        }
        args.push("-c:a".to_owned());
        args.push("copy".to_owned());

        args
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::exports::AudioTrackKind;

  fn tracks(count: usize) -> Vec<RecordingAudioTrack> {
    (0..count)
      .map(|stream_index| RecordingAudioTrack {
        kind: AudioTrackKind::Unknown,
        label: format!("Audio {}", stream_index + 1),
        stream_index,
      })
      .collect()
  }

  #[test]
  fn keeps_the_recordings_own_order_whatever_order_the_window_sent() {
    let selection = TrackSelection::new(&tracks(3), &[2, 0]);

    assert_eq!(selection.signature(), "0-2");
  }

  #[test]
  fn drops_a_track_this_recording_does_not_have() {
    let selection = TrackSelection::new(&tracks(1), &[0, 7]);

    assert_eq!(selection.signature(), "0");
    assert!(selection.covers(&tracks(1)));
  }

  #[test]
  fn names_the_empty_selection_rather_than_leaving_a_blank() {
    let selection = TrackSelection::new(&tracks(2), &[]);

    assert_eq!(selection.signature(), "silent");
    assert_eq!(
      selection.audio_args(AudioLayout::Mixdown),
      vec!["-an".to_owned()]
    );
    assert_eq!(
      selection.audio_args(AudioLayout::SeparateTracks),
      vec!["-an".to_owned()]
    );
  }

  #[test]
  fn copies_a_lone_track_instead_of_re_encoding_it() {
    let selection = TrackSelection::new(&tracks(2), &[1]);

    assert_eq!(
      selection.audio_args(AudioLayout::Mixdown),
      ["-map", "0:a:1", "-c:a", "copy"]
    );
  }

  #[test]
  fn sums_without_letting_amix_halve_either_track() {
    let selection = TrackSelection::new(&tracks(2), &[0, 1]);

    assert_eq!(
      selection.audio_args(AudioLayout::Mixdown),
      [
        "-filter_complex",
        "[0:a:0][0:a:1]amix=inputs=2:normalize=0[mix]",
        "-map",
        "[mix]",
        "-c:a",
        "aac",
        "-b:a",
        MIXDOWN_BITRATE,
      ]
    );
  }

  #[test]
  fn keeps_every_selected_track_separate_for_an_export() {
    let selection = TrackSelection::new(&tracks(3), &[0, 2]);

    assert_eq!(
      selection.audio_args(AudioLayout::SeparateTracks),
      ["-map", "0:a:0", "-map", "0:a:2", "-c:a", "copy"]
    );
  }

  #[test]
  fn knows_when_something_was_left_out() {
    assert!(!TrackSelection::new(&tracks(2), &[0]).covers(&tracks(2)));
    assert!(TrackSelection::new(&tracks(2), &[0, 1]).covers(&tracks(2)));
  }

  #[test]
  fn only_processes_an_export_when_its_contents_or_layout_change() {
    let all = TrackSelection::new(&tracks(2), &[0, 1]);
    assert!(!all.needs_processing(&tracks(2), AudioLayout::SeparateTracks));
    assert!(all.needs_processing(&tracks(2), AudioLayout::Mixdown));

    let one = TrackSelection::new(&tracks(2), &[0]);
    assert!(one.needs_processing(&tracks(2), AudioLayout::SeparateTracks));
    assert!(one.needs_processing(&tracks(2), AudioLayout::Mixdown));

    let only = TrackSelection::new(&tracks(1), &[0]);
    assert!(!only.needs_processing(&tracks(1), AudioLayout::Mixdown));
  }

  #[test]
  fn estimates_selected_and_collapsed_audio_from_their_real_bitrates() {
    let typed = vec![
      RecordingAudioTrack {
        kind: AudioTrackKind::SystemAudio,
        label: "System audio".to_owned(),
        stream_index: 0,
      },
      RecordingAudioTrack {
        kind: AudioTrackKind::Microphone,
        label: "Microphone".to_owned(),
        stream_index: 1,
      },
    ];
    let both = TrackSelection::new(&typed, &[0, 1]);
    assert_eq!(
      both.estimated_audio_bytes(&typed, AudioLayout::SeparateTracks, 10_000),
      400_000
    );
    assert_eq!(
      both.estimated_audio_bytes(&typed, AudioLayout::Mixdown, 10_000),
      240_000
    );

    let microphone = TrackSelection::new(&typed, &[1]);
    assert_eq!(
      microphone.estimated_audio_bytes(&typed, AudioLayout::SeparateTracks, 10_000),
      160_000
    );
  }
}
