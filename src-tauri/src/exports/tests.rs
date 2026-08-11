// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{
  recovery::{
    camera_for_recording, cursor_for_recording, orphaned_recordings, sweep_preview_files,
    sweep_unclaimed_cameras, sweep_unclaimed_cursors, OrphanPlan,
  },
  save::{save_recording, save_selected_recording},
  *,
};

fn valid_cursor_sidecar(path: &Path) {
  let header = crate::recording::cursor::CursorRecord::Header {
    coordinate_space: "global-logical-points".to_owned(),
    platform: "test".to_owned(),
    source: crate::recording::cursor::CursorSource {
      height: 100.0,
      kind: crate::recording::cursor::CursorSourceKind::Screen,
      platform_id: "1".to_owned(),
      video_height: 200,
      video_width: 200,
      width: 100.0,
      x: 0.0,
      y: 0.0,
    },
    timebase: "recording-microseconds".to_owned(),
    version: crate::recording::cursor::FORMAT_VERSION,
  };
  std::fs::write(
    path,
    format!("{}\n", serde_json::to_string(&header).unwrap()),
  )
  .unwrap();
}

#[test]
fn keeps_a_reasonable_name_untouched() {
  assert_eq!(
    sanitize_file_stem("Orbit Capture 2026-08-08 at 14.32.05").as_deref(),
    Some("Orbit Capture 2026-08-08 at 14.32.05")
  );
}

#[test]
fn strips_characters_neither_platform_allows() {
  assert_eq!(
    sanitize_file_stem(r#"a<b>c:d"e/f\g|h?i*j"#).as_deref(),
    Some("abcdefghij")
  );
}

#[test]
fn strips_control_characters() {
  assert_eq!(
    sanitize_file_stem("one\ttwo\nthree").as_deref(),
    Some("onetwothree")
  );
}

#[test]
fn trims_surrounding_whitespace() {
  assert_eq!(sanitize_file_stem("   shot   ").as_deref(), Some("shot"));
}

#[test]
fn drops_trailing_dots_and_spaces_that_windows_would_eat() {
  assert_eq!(sanitize_file_stem("shot. . .").as_deref(), Some("shot"));
  assert_eq!(sanitize_file_stem("shot   ").as_deref(), Some("shot"));
}

#[test]
fn rejects_a_name_with_nothing_left_in_it() {
  assert_eq!(sanitize_file_stem(""), None);
  assert_eq!(sanitize_file_stem("   "), None);
  assert_eq!(sanitize_file_stem("///"), None);
  assert_eq!(sanitize_file_stem("..."), None);
}

#[test]
fn rejects_names_windows_reserves() {
  assert_eq!(sanitize_file_stem("CON"), None);
  assert_eq!(sanitize_file_stem("nul"), None);
  assert_eq!(sanitize_file_stem("Com1"), None);
  assert_eq!(sanitize_file_stem("LPT9"), None);
  // Only the exact stem is reserved.
  assert_eq!(sanitize_file_stem("console").as_deref(), Some("console"));
}

#[test]
fn caps_an_absurdly_long_name() {
  let stem = sanitize_file_stem(&"a".repeat(500)).unwrap();
  assert_eq!(stem.len(), MAX_FILE_STEM);
}

#[test]
fn tells_a_preview_from_a_recording_however_far_it_got() {
  let directory = std::env::temp_dir()
    .join("orbit-capture-tests")
    .join("preview-sweep");
  let _ = std::fs::remove_dir_all(&directory);
  std::fs::create_dir_all(&directory).unwrap();

  let recording = directory.join("recording-20260808-143205.000.mov");
  let mix = directory.join("preview-42-7-mix-0-1.mp4");
  let abandoned = directory.join("preview-42-7-mix-0-1.mp4.3.part");
  for path in [&recording, &mix, &abandoned] {
    std::fs::write(path, b"movie").unwrap();
  }

  // Neither derivative may be offered back as the recording an earlier run
  // never saved - one is a mixdown, the other is not even a whole file.
  let orphans = orphaned_recordings(&directory);
  assert_eq!(
    orphans.iter().map(|(path, _)| path).collect::<Vec<_>>(),
    vec![&recording]
  );

  // Both go at startup, though: an interrupted encode is as worthless as a
  // finished mix once the artifact it belonged to is gone.
  sweep_preview_files(&directory);
  assert!(recording.is_file());
  assert!(!mix.exists());
  assert!(!abandoned.exists());
}

/// A directory of this test module's own, so a test that writes files cannot
/// be confused by anything else on the machine.
fn test_directory(name: &str) -> PathBuf {
  let directory = std::env::temp_dir().join("orbit-capture-tests").join(name);
  let _ = std::fs::remove_dir_all(&directory);
  std::fs::create_dir_all(&directory).unwrap();
  directory
}

#[test]
fn recovers_an_unsaved_recording_whichever_container_it_was_written_in() {
  let directory = test_directory("orphan-containers");

  // What this version writes, and what the version before it wrote. Someone
  // who upgraded with an unsaved recording still on disk has the second.
  let quicktime = directory.join("recording-20260808-143205.000.mov");
  let audio = directory.join("audio-20260808-153205.000.mov");
  let legacy = directory.join("recording-20260807-091500.000.mp4");
  // Case is the file system's business, not ours.
  let shouted = directory.join("recording-20260806-091500.000.MOV");
  let unrelated = directory.join("notes.txt");
  for path in [&quicktime, &audio, &legacy, &shouted, &unrelated] {
    std::fs::write(path, b"movie").unwrap();
  }

  let mut found: Vec<PathBuf> = orphaned_recordings(&directory)
    .into_iter()
    .map(|(path, _)| path)
    .collect();
  found.sort();
  let mut expected = vec![quicktime, audio, legacy, shouted];
  expected.sort();

  assert_eq!(found, expected);
}

#[test]
fn describes_a_recording_by_the_file_the_user_will_actually_get() {
  let working = Path::new("/tmp/recording-20260808-143205.000.mov");
  assert_eq!(delivered_extension(working, true), "mp4");
  // Nothing to copy it with, so what is offered is the movie itself - never
  // that movie under a name it does not answer to.
  assert_eq!(delivered_extension(working, false), "mov");
  // A recording recovered from a version that wrote .mp4 is already what it
  // would have been remuxed into.
  assert_eq!(
    delivered_extension(Path::new("/tmp/recording-1.mp4"), false),
    "mp4"
  );
}

#[test]
fn accepts_only_the_camera_resolution_choices_the_window_offers() {
  for scale in [50, 75, 100] {
    assert!(validate_camera_resolution_scale(scale).is_ok());
  }
  for scale in [0, 49, 76, 101] {
    assert!(validate_camera_resolution_scale(scale).is_err());
  }
}

#[test]
fn accepts_only_camera_overlay_values_the_window_can_produce() {
  let valid = CameraOverlaySettings {
    camera_x_percent: 50.0,
    camera_y_percent: 50.0,
    camera_width_percent: 60.0,
    frame_height_percent: 40.0,
    frame_width_percent: 60.0,
    frame_x_percent: 40.0,
    frame_y_percent: 30.0,
    radius_percent: 50.0,
  };
  assert!(validate_camera_overlay(valid).is_ok());
  assert!(validate_camera_overlay(CameraOverlaySettings {
    camera_width_percent: 2.0,
    ..valid
  })
  .is_err());
  assert!(validate_camera_overlay(CameraOverlaySettings {
    frame_width_percent: 61.0,
    ..valid
  })
  .is_err());
  assert!(validate_camera_overlay(CameraOverlaySettings {
    camera_x_percent: f64::NAN,
    ..valid
  })
  .is_err());
}

/// Stands in for a stream copy that works, without needing FFmpeg to be on
/// the machine running the test.
fn copies(source: &Path, destination: &Path) -> Result<(), String> {
  std::fs::copy(source, destination)
    .map(|_| ())
    .map_err(|error| error.to_string())
}

fn refuses(_: &Path, _: &Path) -> Result<(), String> {
  Err("no".to_owned())
}

fn copies_selected(
  source: &Path,
  destination: &Path,
  _: &track_selection::TrackSelection,
  _: track_selection::AudioLayout,
  _: media_preview::ExportRunOptions<'_>,
) -> Result<media_preview::ExportRunResult, String> {
  copies(source, destination).map(|()| media_preview::ExportRunResult::Completed)
}

fn refuses_selected(
  _: &Path,
  _: &Path,
  _: &track_selection::TrackSelection,
  _: track_selection::AudioLayout,
  _: media_preview::ExportRunOptions<'_>,
) -> Result<media_preview::ExportRunResult, String> {
  Err("no".to_owned())
}

#[test]
fn saves_a_recording_as_an_mp4_when_it_can_be_copied_into_one() {
  let directory = test_directory("save-remuxed");
  let working = directory.join("recording-20260808-143205.000.mov");
  std::fs::write(&working, b"movie").unwrap();

  let saved = save_recording(&working, &directory, "Keeper", Some(copies)).unwrap();

  assert_eq!(saved, directory.join("Keeper.mp4"));
  assert!(saved.is_file());
  // The working file is let go of only once its replacement exists.
  assert!(!working.exists());
}

#[test]
fn saves_a_recording_as_the_movie_it_is_when_there_is_nothing_to_copy_it_with() {
  let directory = test_directory("save-without-ffmpeg");
  let working = directory.join("recording-20260808-143205.000.mov");
  std::fs::write(&working, b"movie").unwrap();

  let saved = save_recording(&working, &directory, "Keeper", None).unwrap();

  // A .mov named .mp4 is a file that lies about itself, so the honest name
  // is the one the user gets.
  assert_eq!(saved, directory.join("Keeper.mov"));
  assert!(saved.is_file());
  assert!(!directory.join("Keeper.mp4").exists());
  assert!(!working.exists());
}

#[test]
fn saves_a_recording_as_the_movie_it_is_when_the_copy_fails() {
  let directory = test_directory("save-failed-remux");
  let working = directory.join("recording-20260808-143205.000.mov");
  std::fs::write(&working, b"movie").unwrap();

  let saved = save_recording(&working, &directory, "Keeper", Some(refuses)).unwrap();

  // FFmpeg refusing the file is no reason to lose a recording someone just
  // asked to keep.
  assert_eq!(saved, directory.join("Keeper.mov"));
  assert!(saved.is_file());
  assert!(!directory.join("Keeper.mp4").exists());
}

#[test]
fn saves_beside_a_name_that_is_already_taken() {
  let directory = test_directory("save-collision");
  let working = directory.join("recording-20260808-143205.000.mov");
  std::fs::write(&working, b"movie").unwrap();
  std::fs::write(directory.join("Keeper.mp4"), b"someone else's").unwrap();

  let saved = save_recording(&working, &directory, "Keeper", Some(copies)).unwrap();

  assert_eq!(saved, directory.join("Keeper (2).mp4"));
}

#[test]
fn saves_a_selected_audio_layout_without_changing_the_working_movie() {
  let directory = test_directory("save-selected-audio");
  let working = directory.join("recording-20260808-143205.000.mov");
  std::fs::write(&working, b"movie").unwrap();
  let tracks = recording_audio_tracks(true, true);
  let selection = track_selection::TrackSelection::new(&tracks, &[1]);
  let cancelled = AtomicBool::new(false);
  let mut ignore_progress = |_| {};

  let saved = save_selected_recording(
    &working,
    &directory,
    "Keeper",
    &selection,
    track_selection::AudioLayout::SeparateTracks,
    media_preview::ExportRunOptions {
      cancelled: &cancelled,
      on_progress: &mut ignore_progress,
      video: media_preview::VideoExportOptions {
        compression: 0,
        resolution_scale_percent: 200,
        source_scale_percent: 200,
      },
    },
    Some(copies_selected),
  )
  .unwrap();

  assert_eq!(saved, Some(directory.join("Keeper.mp4")));
  assert!(!working.exists());
}

#[test]
fn keeps_the_working_movie_when_a_selected_audio_export_fails() {
  let directory = test_directory("save-selected-audio-failure");
  let working = directory.join("recording-20260808-143205.000.mov");
  std::fs::write(&working, b"movie").unwrap();
  let tracks = recording_audio_tracks(true, true);
  let selection = track_selection::TrackSelection::new(&tracks, &[1]);
  let cancelled = AtomicBool::new(false);
  let mut ignore_progress = |_| {};

  assert!(save_selected_recording(
    &working,
    &directory,
    "Keeper",
    &selection,
    track_selection::AudioLayout::SeparateTracks,
    media_preview::ExportRunOptions {
      cancelled: &cancelled,
      on_progress: &mut ignore_progress,
      video: media_preview::VideoExportOptions {
        compression: 0,
        resolution_scale_percent: 200,
        source_scale_percent: 200,
      },
    },
    Some(refuses_selected),
  )
  .is_err());
  assert!(working.exists());
  assert!(!directory.join("Keeper.mp4").exists());
}

/// The one test here that uses the real stream copy. It is skipped rather
/// than failed on a machine without FFmpeg, because that machine is exactly
/// the one the fallback above exists for.
#[test]
fn carries_every_recorded_track_into_the_saved_mp4() {
  let Some(remux) = media_preview::remuxer() else {
    eprintln!("skipped: FFmpeg is not on this machine");
    return;
  };

  let directory = test_directory("save-real-remux");
  let working = directory.join("recording-20260808-143205.000.mov");
  // A picture and two audio tracks, which is what a recording with both
  // system audio and a microphone carries.
  let built = std::process::Command::new("ffmpeg")
    .args([
      "-hide_banner",
      "-loglevel",
      "error",
      "-y",
      "-f",
      "lavfi",
      "-i",
      "testsrc=size=320x240:rate=30:duration=1",
      "-f",
      "lavfi",
      "-i",
      "sine=frequency=440:duration=1",
      "-f",
      "lavfi",
      "-i",
      "sine=frequency=880:duration=1",
      "-c:v",
      "libx264",
      "-c:a",
      "aac",
      "-map",
      "0:v",
      "-map",
      "1:a",
      "-map",
      "2:a",
    ])
    .arg(&working)
    .status();
  if !built.is_ok_and(|status| status.success()) {
    eprintln!("skipped: this FFmpeg could not build the source movie");
    return;
  }

  let saved = save_recording(&working, &directory, "Keeper", Some(remux)).unwrap();
  assert_eq!(saved, directory.join("Keeper.mp4"));

  // Three streams in, three streams out. Dropping the second audio track
  // here would be silent data loss, which is the whole reason the copy maps
  // every stream rather than the first of each kind.
  assert_eq!(streams(&saved), 3);
}

/// How many streams a file holds, read out of what FFmpeg prints about it.
fn streams(path: &Path) -> usize {
  let output = std::process::Command::new("ffmpeg")
    .args(["-hide_banner", "-nostdin", "-i"])
    .arg(path)
    .output()
    .unwrap();

  String::from_utf8_lossy(&output.stderr)
    .lines()
    .filter(|line| line.trim_start().starts_with("Stream #"))
    .count()
}

const NOW: SystemTime = SystemTime::UNIX_EPOCH;

fn aged(name: &str, ago: Duration) -> (PathBuf, SystemTime) {
  (PathBuf::from(name), NOW - ago)
}

#[test]
fn offers_back_the_newest_unsaved_recording() {
  let plan = orphan_plan(
    vec![
      aged("/tmp/old.mov", Duration::from_secs(3_600)),
      aged("/tmp/newest.mov", Duration::from_secs(60)),
      aged("/tmp/middle.mov", Duration::from_secs(600)),
    ],
    NOW,
  );

  assert_eq!(plan.present.as_deref(), Some(Path::new("/tmp/newest.mov")));
  // Still inside their keeping age, so a later run can still offer them.
  assert!(plan.delete.is_empty());
}

#[test]
fn sweeps_away_anything_past_its_keeping_age() {
  let plan = orphan_plan(
    vec![
      aged("/tmp/ancient.mov", ORPHAN_MAX_AGE + Duration::from_secs(1)),
      aged("/tmp/recent.mov", Duration::from_secs(60)),
    ],
    NOW,
  );

  assert_eq!(plan.delete, vec![PathBuf::from("/tmp/ancient.mov")]);
  assert_eq!(plan.present.as_deref(), Some(Path::new("/tmp/recent.mov")));
}

#[test]
fn offers_nothing_back_when_everything_is_too_old() {
  let plan = orphan_plan(vec![aged("/tmp/ancient.mov", ORPHAN_MAX_AGE * 2)], NOW);

  assert_eq!(plan.present, None);
  assert_eq!(plan.delete.len(), 1);
}

#[test]
fn keeps_a_recording_stamped_in_the_future() {
  // A clock that moved backwards makes an age impossible to read, and
  // deleting someone's recording is the worse half of that guess.
  let plan = orphan_plan(
    vec![(
      PathBuf::from("/tmp/ahead.mov"),
      NOW + Duration::from_secs(60),
    )],
    NOW,
  );

  assert_eq!(plan.present.as_deref(), Some(Path::new("/tmp/ahead.mov")));
  assert!(plan.delete.is_empty());
}

#[test]
fn does_nothing_with_an_empty_directory() {
  assert_eq!(orphan_plan(Vec::new(), NOW), OrphanPlan::default());
}

#[test]
fn pairs_a_camera_sidecar_with_its_recording() {
  let directory = std::env::temp_dir()
    .join("orbit-capture-tests")
    .join("camera-pair");
  let _ = std::fs::remove_dir_all(&directory);
  std::fs::create_dir_all(&directory).unwrap();

  let recording = directory.join("recording-20260809-060151.000.mov");
  let camera = directory.join("camera-20260809-060151.000.mov");
  std::fs::write(&recording, b"screen").unwrap();
  std::fs::write(&camera, b"camera").unwrap();

  assert_eq!(
    camera_for_recording(&recording).as_deref(),
    Some(camera.as_path())
  );

  std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn sweeps_only_unclaimed_camera_sidecars() {
  let directory = std::env::temp_dir()
    .join("orbit-capture-tests")
    .join("camera-sweep");
  let _ = std::fs::remove_dir_all(&directory);
  std::fs::create_dir_all(&directory).unwrap();

  let kept = directory.join("camera-kept.mov");
  let abandoned = directory.join("camera-abandoned.mov");
  let unrelated = directory.join("notes.txt");
  for path in [&kept, &abandoned, &unrelated] {
    std::fs::write(path, b"data").unwrap();
  }

  sweep_unclaimed_cameras(&directory, Some(&kept));

  assert!(kept.exists());
  assert!(!abandoned.exists());
  assert!(unrelated.exists());

  std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn pairs_only_a_valid_cursor_sidecar_with_its_recording() {
  let directory = test_directory("cursor-pair");
  let recording = directory.join("recording-20260809-060151.000.mov");
  let cursor = directory.join("recording-20260809-060151.000.cursor.jsonl");
  std::fs::write(&recording, b"screen").unwrap();
  valid_cursor_sidecar(&cursor);

  assert_eq!(
    cursor_for_recording(&recording).as_deref(),
    Some(cursor.as_path())
  );

  std::fs::write(&cursor, b"not a cursor stream").unwrap();
  assert_eq!(cursor_for_recording(&recording), None);
  std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn sweeps_only_unclaimed_cursor_sidecars() {
  let directory = test_directory("cursor-sweep");
  let kept = directory.join("recording-kept.cursor.jsonl");
  let abandoned = directory.join("recording-abandoned.cursor.jsonl");
  let unrelated = directory.join("notes.jsonl");
  for path in [&kept, &abandoned, &unrelated] {
    std::fs::write(path, b"data").unwrap();
  }

  sweep_unclaimed_cursors(&directory, Some(&kept));

  assert!(kept.exists());
  assert!(!abandoned.exists());
  assert!(unrelated.exists());
  std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn keeps_a_dot_inside_the_name() {
  assert_eq!(
    sanitize_file_stem("v1.2.3 build").as_deref(),
    Some("v1.2.3 build")
  );
}
