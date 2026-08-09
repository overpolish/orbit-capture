// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// Names a mix so that only the process that wrote it ever writes to it.
///
/// Artifact ids are minted from a counter that starts at one in every process,
/// so a second copy of the app - sharing the same recordings directory -
/// numbers its first recording `1` as well and would otherwise arrive at the
/// identical file name. Both would then be encoding onto one path while the
/// other's window played it, which is precisely how a preview ends up with a
/// band of garbage across it and a playhead that stops moving. The process id
/// makes each instance's mixes its own, and both remain reclaimable at startup
/// because the name still begins with the preview prefix.
pub(super) fn mix_path(source: &Path, artifact_id: u64, signature: &str) -> PathBuf {
  mix_path_for(source, std::process::id(), artifact_id, signature)
}

pub(super) fn mix_path_for(
  source: &Path,
  process: u32,
  artifact_id: u64,
  signature: &str,
) -> PathBuf {
  source.with_file_name(format!(
    "{PREVIEW_PREFIX}{process}-{artifact_id}-mix-{signature}.mp4"
  ))
}

/// Counts the mixes this process has attempted, so two of them in flight at
/// once - or one retried after a failure - never share a half-written file.
static MIX_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

/// Where FFmpeg writes while it is still working.
///
/// FFmpeg fills an MP4 progressively and only stamps the index that makes it
/// playable when it finishes, so for the whole length of the encode the file
/// on disk is a truncation. Writing that under the name the window is told to
/// play hands it whatever existed at the moment it looked. The final name is
/// only ever created by a rename, which is atomic within a directory, so the
/// path either does not exist or is a finished file - never something in
/// between.
///
/// The name is a sibling of the destination and inherits its process id, plus
/// an attempt number of its own; the preview prefix carries over from the
/// destination, so the startup sweep reclaims a `.part` left by a crash just
/// as it reclaims a finished mix.
pub(super) fn mix_temp_path(destination: &Path) -> PathBuf {
  let attempt = MIX_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
  let name = destination.file_name().map_or_else(
    || PREVIEW_PREFIX.to_owned(),
    |name| name.to_string_lossy().into_owned(),
  );

  destination.with_file_name(format!("{name}.{attempt}.part"))
}

/// Writes a single file the export window can simply play.
///
/// The picture is stream-copied - always, without exception. This runs on a
/// recording the user is about to keep, and re-encoding it here would cost
/// minutes and quality to produce something nobody keeps. Only the audio is
/// touched, and only because a video element can play one track at a time.
pub(super) fn mix_args(
  source: &Path,
  destination: &Path,
  selection: &TrackSelection,
) -> Vec<OsString> {
  let mut args: Vec<OsString> = ["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"]
    .map(OsString::from)
    .into();
  args.push(source.into());
  args.extend(["-map", "0:v:0", "-c:v", "copy"].map(OsString::from));
  args.extend(
    selection
      .audio_args(AudioLayout::Mixdown)
      .into_iter()
      .map(OsString::from),
  );
  args.extend(PREVIEW_MP4_OUTPUT.map(OsString::from));
  args.push(destination.into());

  args
}

/// What a preview mix is, spelled out rather than left to be guessed. FFmpeg
/// cannot infer a muxer from its temporary `.part` name. `+faststart` puts the
/// index first; signed composition offsets stop it compensating for H.264
/// reordering with a video edit that skips the decoder's initial references.
const PREVIEW_MP4_OUTPUT: [&str; 4] = ["-f", "mp4", "-movflags", "+faststart+negative_cts_offsets"];

/// The saved movie already plays reliably across the supported players, so it
/// deliberately keeps its established muxing flags. The signed composition
/// offsets above are a WebKit preview compatibility measure, not a reason to
/// change a known-good export container.
pub(super) const EXPORT_MP4_OUTPUT: [&str; 4] = ["-f", "mp4", "-movflags", "+faststart"];

/// How much of FFmpeg's complaint is worth carrying back to the window. The
/// interesting line is always the last one; everything before it is context
/// the user cannot act on.
pub(super) const MIX_ERROR_DETAIL: usize = 400;

pub(super) fn mix_error(stderr: &[u8]) -> String {
  const MESSAGE: &str = "FFmpeg could not prepare the preview audio";
  let detail = String::from_utf8_lossy(stderr);
  let detail = detail.trim();
  if detail.is_empty() {
    return MESSAGE.to_owned();
  }

  let tail = detail
    .char_indices()
    .rev()
    .nth(MIX_ERROR_DETAIL - 1)
    .map_or(detail, |(index, _)| &detail[index..]);

  format!("{MESSAGE}: {tail}")
}

/// Whether a finished encode is a file a player can actually open.
///
/// A truncated MP4 has no index, so a probe of its container fails where a
/// look at its size would not - which is the whole point of asking. FFprobe is
/// not a hard dependency, though: if it cannot be started at all, the mix is
/// taken on the size check alone rather than refusing to produce a preview on
/// a machine that only has the encoder.
pub(super) fn plays_from_start_to_end(path: &Path) -> bool {
  let Ok(output) = Command::new(ffprobe_path())
    .args([
      "-v",
      "error",
      "-show_entries",
      "format=duration",
      "-of",
      "default=noprint_wrappers=1:nokey=1",
    ])
    .arg(path)
    .output()
  else {
    return true;
  };

  output.status.success()
    && String::from_utf8_lossy(&output.stdout)
      .trim()
      .parse::<f64>()
      .is_ok_and(|duration| duration > 0.0)
}

/// Whether a file is there and has something in it. Cheap enough to run every
/// time a mix is handed out, unlike [`plays_from_start_to_end`].
pub(super) fn holds_bytes(path: &Path) -> bool {
  std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
}

pub(super) fn mix(
  source: &Path,
  destination: &Path,
  selection: &TrackSelection,
) -> Result<(), String> {
  let temporary = mix_temp_path(destination);
  // Captured rather than inherited: a bundled app has nowhere to print to, and
  // the one line FFmpeg writes when it gives up is the only thing that says
  // why the preview never appeared.
  let output = Command::new(ffmpeg_path())
    .args(mix_args(source, &temporary, selection))
    .output()
    .map_err(|error| {
      let _ = std::fs::remove_file(&temporary);
      format!("FFmpeg could not be started: {error}")
    })?;

  // A failed encode, an empty file and an unopenable one all mean the same
  // thing here: nothing worth publishing under the final name. The temporary
  // goes in every case, so a retry never inherits the last attempt's remains.
  if !output.status.success() || !holds_bytes(&temporary) || !plays_from_start_to_end(&temporary) {
    let _ = std::fs::remove_file(&temporary);
    return Err(mix_error(&output.stderr));
  }

  std::fs::rename(&temporary, destination).map_err(|error| {
    let _ = std::fs::remove_file(&temporary);
    format!("The preview audio could not be put in place: {error}")
  })?;

  Ok(())
}

/// Turns the working QuickTime movie into the .mp4 the user keeps.
///
/// Not a re-encode and not a rename. `-c copy` on `-map 0` carries every
/// stream across untouched - there can be two audio tracks, and dropping the
/// microphone here would be silent data loss - so this costs a read and a
/// write of the file rather than minutes of encoding.
///
/// The recording is written as a QuickTime movie because that is the only
/// container that survives being fragmented, and fragments are what make a
/// crashed recording recoverable. Nothing about that is the user's problem, so
/// what they keep is the .mp4 everything opens.
/// The preview files built so far for the artifact on screen.
///
/// Kept rather than rebuilt because toggling a track off and back on is the
/// most ordinary thing a person does here, and the file for that combination
/// is already on disk. They belong to one artifact at a time: a replacement
/// makes every one of them garbage.
#[derive(Default)]
pub struct PreviewMixes {
  artifact_id: u64,
  pub(super) by_signature: HashMap<String, PathBuf>,
}

impl PreviewMixes {
  pub fn cleanup(&mut self) {
    for (_, path) in self.by_signature.drain() {
      let _ = std::fs::remove_file(path);
    }
    self.artifact_id = 0;
  }

  /// The file for this combination, if it was built and is still worth
  /// playing.
  ///
  /// A remembered name is not a promise: the file behind it can have been
  /// swept, or left empty by an earlier run that died mid-encode. Serving one
  /// of those is worse than rebuilding, because the window plays it once and
  /// then holds a broken preview for as long as the artifact is open, so a
  /// name that no longer stands up is forgotten and the mix is made again.
  /// Only the cheap checks belong here - a probe on every request would cost a
  /// process launch each time a track is toggled.
  pub(super) fn cached(&mut self, artifact_id: u64, signature: &str) -> Option<PathBuf> {
    if self.artifact_id != artifact_id {
      return None;
    }

    let path = self.by_signature.get(signature)?.clone();
    if holds_bytes(&path) {
      return Some(path);
    }

    self.by_signature.remove(signature);
    let _ = std::fs::remove_file(&path);

    None
  }

  pub(super) fn remember(&mut self, artifact_id: u64, signature: String, path: PathBuf) {
    if self.artifact_id != artifact_id {
      self.cleanup();
      self.artifact_id = artifact_id;
    }
    if let Some(replaced) = self.by_signature.insert(signature, path) {
      // Only reachable if the same combination was built twice, in which case
      // the two names are equal and there is nothing to remove.
      let _ = std::fs::remove_file(replaced);
    }
  }
}

/// Whether the recording can be played as it is, or only through a mix.
///
/// A media element renders the *first* audio track of a file and nothing else.
/// WebKit does not even list the others: a two-track recording opened in a
/// WKWebView reports `audioTracks.length` of one, so no amount of enabling
/// tracks from script reaches the second. Measured against a recording whose
/// system-audio track was written first and captured silence, the element
/// played it at `readyState` 4 from end to end, fired `playing` at once, and
/// made no sound whatsoever - the microphone underneath it was simply never
/// rendered.
///
/// So a recording carrying more than one audio track is not a stand-in for its
/// own mix. With one track there is nothing to sum, and stream-copying a
/// multi-gigabyte movie to arrive back at the file we started from would be
/// seconds of work for no difference.
pub const fn plays_without_mixing(tracks: &[RecordingAudioTrack]) -> bool {
  tracks.len() <= 1
}

/// The file the window should play for a given set of enabled tracks.
///
/// Returns the recording itself when no mixing could change what is heard - see
/// [`plays_without_mixing`] for exactly when that is.
pub fn preview_mix(
  mixes: &mut PreviewMixes,
  artifact_id: u64,
  source: &Path,
  tracks: &[RecordingAudioTrack],
  selection: &TrackSelection,
) -> Result<PathBuf, String> {
  if plays_without_mixing(tracks) && selection.covers(tracks) {
    return Ok(source.to_owned());
  }

  let signature = selection.signature();
  if let Some(cached) = mixes.cached(artifact_id, &signature) {
    return Ok(cached);
  }

  let destination = mix_path(source, artifact_id, &signature);
  mix(source, &destination, selection)?;
  mixes.remember(artifact_id, signature, destination.clone());

  Ok(destination)
}
