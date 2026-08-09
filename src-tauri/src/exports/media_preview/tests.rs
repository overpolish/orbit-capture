// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{
  encode::{
    export_selected_recording, progress_milliseconds, remux_args, remux_temp_path,
    selected_export_args,
  },
  estimate::{compression_crf, estimate_filter},
  preview_mix::{mix_args, mix_path_for, mix_temp_path, plays_without_mixing},
  *,
};

/// A directory of this module's own, so a test that writes files cannot be
/// confused by anything else on the machine.
fn test_directory(name: &str) -> PathBuf {
  let directory = std::env::temp_dir().join("orbit-capture-tests").join(name);
  let _ = std::fs::remove_dir_all(&directory);
  std::fs::create_dir_all(&directory).unwrap();
  directory
}

#[test]
fn recognises_its_own_derivatives_by_name() {
  assert!(is_preview_file(Path::new("/tmp/preview-42-7-mix-0-1.mp4")));
  // An abandoned encode too: it is named after the mix it was going to
  // become, so the startup sweep reclaims it without knowing it exists.
  assert!(is_preview_file(Path::new(
    "/tmp/preview-42-7-mix-0-1.mp4.3.part"
  )));
  assert!(!is_preview_file(Path::new(
    "/tmp/recording-20260808-143205.000.mp4"
  )));
}

#[test]
fn names_a_mix_after_the_combination_it_holds() {
  let source = Path::new("/tmp/recording-123.mp4");
  assert_eq!(
    mix_path_for(source, 42, 7, "0-1"),
    Path::new("/tmp/preview-42-7-mix-0-1.mp4")
  );
}

#[test]
fn keeps_two_instances_off_one_another_s_mixes() {
  let source = Path::new("/tmp/recording-123.mp4");
  // Both processes are on their first artifact and the same combination of
  // tracks; only the process id keeps them apart.
  assert_ne!(
    mix_path_for(source, 42, 1, "0-1"),
    mix_path_for(source, 43, 1, "0-1")
  );
}

#[test]
fn encodes_beside_the_mix_rather_than_onto_it() {
  let destination = mix_path_for(Path::new("/tmp/recording-123.mp4"), 42, 7, "0-1");
  let temporary = mix_temp_path(&destination);

  assert_ne!(temporary, destination);
  assert_eq!(temporary.parent(), destination.parent());
  assert!(is_preview_file(&temporary));
  assert_eq!(temporary.extension().unwrap(), "part");
  // Two encodes in flight at once still write to files of their own.
  assert_ne!(mix_temp_path(&destination), temporary);
}

#[test]
fn plays_a_remembered_mix_only_while_it_stands_up() {
  let directory = test_directory("preview-mix-cache");
  let healthy = directory.join("preview-42-7-mix-0.mp4");
  let empty = directory.join("preview-42-7-mix-1.mp4");
  let missing = directory.join("preview-42-7-mix-2.mp4");
  std::fs::write(&healthy, b"not really a movie, but not nothing").unwrap();
  std::fs::write(&empty, b"").unwrap();

  let mut mixes = PreviewMixes::default();
  mixes.remember(7, "0".to_owned(), healthy.clone());
  mixes.remember(7, "1".to_owned(), empty.clone());
  mixes.remember(7, "2".to_owned(), missing.clone());

  assert_eq!(mixes.cached(7, "0"), Some(healthy));
  // A truncation left by an interrupted encode is not a preview, and neither
  // is a name whose file has been swept from under it.
  assert_eq!(mixes.cached(7, "1"), None);
  assert_eq!(mixes.cached(7, "2"), None);
  // Forgotten as well as refused, so the next request rebuilds them.
  assert!(!mixes.by_signature.contains_key("1"));
  assert!(!mixes.by_signature.contains_key("2"));
  assert!(!empty.exists());
}

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
fn lets_a_recording_stand_in_for_itself_only_while_one_track_can_be_heard() {
  assert!(plays_without_mixing(&tracks(0)));
  assert!(plays_without_mixing(&tracks(1)));
  // The second track would be inaudible in a media element, so the mix is
  // the only thing that carries what was recorded.
  assert!(!plays_without_mixing(&tracks(2)));
  assert!(!plays_without_mixing(&tracks(3)));
}

#[test]
fn writes_one_timeline_the_picture_is_copied_onto() {
  let both = tracks(2);
  let selection = TrackSelection::new(&both, &[0, 1]);

  assert_eq!(
    mix_args(
      Path::new("/tmp/recording-123.mp4"),
      Path::new("/tmp/out.mp4"),
      &selection
    ),
    [
      "-hide_banner",
      "-loglevel",
      "error",
      "-nostdin",
      "-y",
      "-i",
      "/tmp/recording-123.mp4",
      "-map",
      "0:v:0",
      "-c:v",
      "copy",
      "-filter_complex",
      "[0:a:0][0:a:1]amix=inputs=2:normalize=0[mix]",
      "-map",
      "[mix]",
      "-c:a",
      "aac",
      "-b:a",
      "192k",
      // Named rather than guessed: the file this is written to is a `.part`,
      // and FFmpeg will not start without being told what to make of it.
      "-f",
      "mp4",
      "-movflags",
      // Signed composition offsets keep FFmpeg from inserting an edit that
      // starts after the H.264 decoder's initial reference frames. WebKit is
      // far less forgiving of that edit than ordinary movie players.
      "+faststart+negative_cts_offsets",
      "/tmp/out.mp4",
    ]
    .map(OsString::from)
  );
}

#[test]
fn copies_every_stream_of_the_recording_into_the_saved_movie() {
  assert_eq!(
    remux_args(
      Path::new("/tmp/recording-123.mov"),
      Path::new("/tmp/Keeper.mp4")
    ),
    [
      "-hide_banner",
      "-loglevel",
      "error",
      "-nostdin",
      "-y",
      "-i",
      "/tmp/recording-123.mov",
      // `-map 0` rather than one stream of each kind: a recording can carry
      // system audio *and* a microphone, and losing one of them here would
      // be silent. `-c copy` because the encode already happened.
      "-map",
      "0",
      "-c",
      "copy",
      "-f",
      "mp4",
      "-movflags",
      "+faststart",
      "/tmp/Keeper.mp4",
    ]
    .map(OsString::from)
  );
}

#[test]
fn maps_only_the_selected_tracks_when_the_saved_audio_changes() {
  let available = tracks(3);
  let selection = TrackSelection::new(&available, &[0, 2]);

  assert_eq!(
    selected_export_args(
      Path::new("/tmp/recording-123.mov"),
      Path::new("/tmp/Keeper.mp4"),
      &selection,
      AudioLayout::SeparateTracks,
      VideoExportOptions {
        compression: 0,
        resolution_scale_percent: 200,
        source_scale_percent: 200,
      },
    ),
    [
      "-hide_banner",
      "-loglevel",
      "error",
      "-nostdin",
      "-y",
      "-i",
      "/tmp/recording-123.mov",
      "-progress",
      "pipe:1",
      "-nostats",
      "-map",
      "0:v:0?",
      "-c:v",
      "copy",
      "-map",
      "0:a:0",
      "-map",
      "0:a:2",
      "-c:a",
      "copy",
      "-f",
      "mp4",
      "-movflags",
      "+faststart",
      "/tmp/Keeper.mp4",
    ]
    .map(OsString::from)
  );
}

#[test]
fn maps_compression_to_a_cross_platform_quality_encode() {
  let available = tracks(1);
  let selection = TrackSelection::new(&available, &[0]);

  let args = selected_export_args(
    Path::new("/tmp/recording.mov"),
    Path::new("/tmp/Keeper.mp4"),
    &selection,
    AudioLayout::SeparateTracks,
    VideoExportOptions {
      compression: 2,
      resolution_scale_percent: 200,
      source_scale_percent: 200,
    },
  );
  let args = args
    .iter()
    .map(|argument| argument.to_string_lossy())
    .collect::<Vec<_>>();

  assert!(args.windows(2).any(|pair| pair == ["-c:v", "libx264"]));
  assert!(args.windows(2).any(|pair| pair == ["-crf", "24"]));
  assert!(args.windows(2).any(|pair| pair == ["-preset", "medium"]));
}

#[test]
fn maps_the_compression_edges_without_reencoding_original() {
  assert_eq!(compression_crf(0), None);
  assert_eq!(compression_crf(1), Some(20));
  assert_eq!(compression_crf(2), Some(24));
  assert_eq!(compression_crf(3), Some(28));
  assert_eq!(compression_crf(4), Some(32));
  assert_eq!(compression_crf(255), Some(32));
}

#[test]
fn downsampling_uses_lanczos_and_requires_an_encode() {
  assert_eq!(resolution_filter(200, 200), None);
  assert_eq!(
    resolution_filter(200, 100).as_deref(),
    Some("scale=trunc(iw*100/200/2)*2:trunc(ih*100/200/2)*2:flags=lanczos")
  );
  assert_eq!(export_crf(0, true), Some(20));
}

#[test]
fn estimate_joins_seeked_samples_before_one_encode() {
  assert_eq!(
    estimate_filter(3, Some("scale=iw/2:ih/2")),
    "[0:v:0]setpts=PTS-STARTPTS[sample0];[1:v:0]setpts=PTS-STARTPTS[sample1];[2:v:0]setpts=PTS-STARTPTS[sample2];[sample0][sample1][sample2]concat=n=3:v=1:a=0,scale=iw/2:ih/2[estimated]"
  );
  assert_eq!(
    estimate_filter(1, None),
    "[0:v:0]setpts=PTS-STARTPTS[estimated]"
  );
}

#[test]
fn reads_ffmpeg_progress_as_milliseconds() {
  assert_eq!(progress_milliseconds("out_time_us=1234567"), Some(1_234));
  assert_eq!(progress_milliseconds("progress=continue"), None);
  assert_eq!(progress_milliseconds("out_time_us=N/A"), None);
}

#[test]
fn estimates_and_compresses_a_real_movie_when_x264_is_available() {
  if !supports_compression() {
    eprintln!("skipped: this FFmpeg does not include libx264");
    return;
  }

  let directory = test_directory("compressed-export");
  let source = directory.join("source.mov");
  let destination = directory.join("compressed.mp4");
  let built = Command::new(ffmpeg_path())
    .args([
      "-hide_banner",
      "-loglevel",
      "error",
      "-y",
      "-f",
      "lavfi",
      "-i",
      "testsrc2=size=320x240:rate=30:duration=3",
      "-f",
      "lavfi",
      "-i",
      "sine=frequency=440:duration=3",
      "-f",
      "lavfi",
      "-i",
      "sine=frequency=880:duration=3",
      "-map",
      "0:v",
      "-map",
      "1:a",
      "-map",
      "2:a",
      "-c:v",
      "libx264",
      "-crf",
      "18",
      "-c:a",
      "aac",
    ])
    .arg(&source)
    .status();
  if !built.is_ok_and(|status| status.success()) {
    eprintln!("skipped: FFmpeg could not build the test movie");
    return;
  }

  let available = tracks(2);
  let selection = TrackSelection::new(&available, &[0, 1]);
  let estimated = estimate_compressed_video_bytes(&source, 3_000, 2, 200, 100).unwrap();
  assert!(estimated > 0);
  let cancelled = AtomicBool::new(false);
  let mut progress = Vec::new();
  export_selected_recording(
    &source,
    &destination,
    &selection,
    AudioLayout::Mixdown,
    ExportRunOptions {
      cancelled: &cancelled,
      on_progress: &mut |milliseconds| progress.push(milliseconds),
      video: VideoExportOptions {
        compression: 2,
        resolution_scale_percent: 100,
        source_scale_percent: 200,
      },
    },
  )
  .unwrap();

  assert!(holds_bytes(&source));
  assert!(plays_from_start_to_end(&destination));
  assert!(progress
    .last()
    .is_some_and(|milliseconds| *milliseconds > 0));
  let output = Command::new(ffmpeg_path())
    .args(["-hide_banner", "-i"])
    .arg(&destination)
    .output()
    .unwrap();
  assert!(String::from_utf8_lossy(&output.stderr).contains("160x120"));
  let estimated_audio = selection.estimated_audio_bytes(&available, AudioLayout::Mixdown, 3_000);
  let estimated_media = estimated.saturating_add(estimated_audio);
  let predicted = estimated_media
    .saturating_add(estimated_media / 200)
    .saturating_add(4_096);
  let actual = std::fs::metadata(&destination).unwrap().len();
  assert!(predicted.abs_diff(actual) <= actual * 2 / 5);
  let described = Command::new(ffmpeg_path())
    .args(["-hide_banner", "-nostdin", "-i"])
    .arg(&destination)
    .output()
    .unwrap();
  let streams = String::from_utf8_lossy(&described.stderr)
    .lines()
    .filter(|line| line.trim_start().starts_with("Stream #"))
    .count();
  // One re-encoded video stream and the two selected audio streams mixed to
  // one, which verifies both choices are applied by the same output pass.
  assert_eq!(streams, 2);

  let cancelled_destination = directory.join("cancelled.mp4");
  let cancelled = AtomicBool::new(true);
  let result = export_selected_recording(
    &source,
    &cancelled_destination,
    &selection,
    AudioLayout::Mixdown,
    ExportRunOptions {
      cancelled: &cancelled,
      on_progress: &mut |_| {},
      video: VideoExportOptions {
        compression: 2,
        resolution_scale_percent: 100,
        source_scale_percent: 200,
      },
    },
  )
  .unwrap();
  assert_eq!(result, ExportRunResult::Cancelled);
  assert!(holds_bytes(&source));
  assert!(!cancelled_destination.exists());
}

#[test]
fn copies_beside_the_saved_movie_rather_than_onto_it() {
  let destination = Path::new("/tmp/Keeper.mp4");
  let temporary = remux_temp_path(destination);

  assert_ne!(temporary, destination);
  // A sibling, so the rename that publishes it cannot cross a volume - the
  // destination is wherever the user chose, which is often an external disk.
  assert_eq!(temporary.parent(), destination.parent());
  assert_eq!(temporary.extension().unwrap(), "part");
  // Not something the user has to look at while it is being written.
  assert!(temporary
    .file_name()
    .unwrap()
    .to_string_lossy()
    .starts_with('.'));
  // Two saves at once still write to files of their own.
  assert_ne!(remux_temp_path(destination), temporary);
}
