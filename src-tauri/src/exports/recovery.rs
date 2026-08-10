// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// What to do with the recordings found in the working directory at startup.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct OrphanPlan {
  pub delete: Vec<PathBuf>,
  pub present: Option<PathBuf>,
}

/// Decides the fate of every recording left behind by a previous run.
///
/// A recording is only ever in the working directory because it was never
/// saved - the app quit, or crashed, between finishing and saving. The most
/// recent one is worth offering back, because it is almost certainly the one
/// that was on screen when that happened. Anything past its keeping age goes,
/// including the newest, so a machine that crashed a month ago does not
/// resurrect a recording nobody remembers making.
pub fn orphan_plan(entries: Vec<(PathBuf, SystemTime)>, now: SystemTime) -> OrphanPlan {
  let (fresh, stale): (Vec<_>, Vec<_>) = entries.into_iter().partition(|(_, modified)| {
    now
      .duration_since(*modified)
      .is_ok_and(|age| age <= ORPHAN_MAX_AGE)
      // A file stamped in the future has no believable age; keeping it is the
      // safer half of the guess.
      || modified > &now
  });

  OrphanPlan {
    delete: stale.into_iter().map(|(path, _)| path).collect(),
    present: fresh
      .into_iter()
      .max_by_key(|(_, modified)| *modified)
      .map(|(path, _)| path),
  }
}

pub(super) fn orphaned_recordings(directory: &Path) -> Vec<(PathBuf, SystemTime)> {
  let Ok(entries) = std::fs::read_dir(directory) else {
    return Vec::new();
  };

  entries
    .filter_map(|entry| {
      let path = entry.ok()?.path();
      // A mixed preview is an `.mp4` in this same folder, and offering one
      // back as an unsaved recording would hand the user a derivative in place
      // of what they actually recorded.
      if media_preview::is_preview_file(&path) {
        return None;
      }
      if !path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("recording-"))
      {
        return None;
      }
      let extension = path.extension()?;
      if WORKING_RECORDING_EXTENSIONS
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
      {
        Some((
          path.clone(),
          std::fs::metadata(&path).ok()?.modified().ok()?,
        ))
      } else {
        None
      }
    })
    .collect()
}

pub(super) fn camera_for_recording(recording: &Path) -> Option<PathBuf> {
  let name = recording.file_name()?.to_str()?;
  let suffix = name.strip_prefix("recording-")?;
  let camera = recording.with_file_name(format!("camera-{suffix}"));
  camera.is_file().then_some(camera)
}

pub(super) fn sweep_unclaimed_cameras(directory: &Path, keep: Option<&Path>) {
  let Ok(entries) = std::fs::read_dir(directory) else {
    return;
  };
  for entry in entries.flatten() {
    let path = entry.path();
    let is_camera = path
      .file_name()
      .and_then(|name| name.to_str())
      .is_some_and(|name| name.starts_with("camera-"));
    if is_camera && keep != Some(path.as_path()) {
      let _ = std::fs::remove_file(path);
    }
  }
}

/// Offers back the recording an earlier run never got to save.
///
/// Deliberately not the whole artifact: the poster and the duration lived in
/// the frames, which are long gone. The name and the file are what matter, and
/// the window renders without a poster rather than pretending to have one.
/// Deletes every preview derivative left in the working directory.
///
/// They exist only for as long as an artifact is on screen, so at startup
/// there is no such thing as one worth keeping: any that are there were
/// stranded by a crash, and each is a copy of a movie sitting in the app's own
/// data directory where nobody will ever look for it.
///
/// The match is on the name's prefix rather than its extension, which is what
/// makes it reach the `.part` files a mix encodes into as well: those are
/// named after the mix they were going to become, so an encode killed halfway
/// is reclaimed here without this needing to know anything about it.
pub(super) fn sweep_preview_files(directory: &Path) {
  let Ok(entries) = std::fs::read_dir(directory) else {
    return;
  };

  for entry in entries.flatten() {
    let path = entry.path();
    if media_preview::is_preview_file(&path) {
      let _ = std::fs::remove_file(path);
    }
  }
}

pub(super) fn sweep_orphaned_recordings(app: &AppHandle) {
  let Ok(directory) = crate::recording::recordings_directory(app) else {
    return;
  };
  sweep_preview_files(&directory);
  let plan = orphan_plan(orphaned_recordings(&directory), SystemTime::now());

  for path in plan.delete {
    let _ = std::fs::remove_file(path);
  }
  let Some(path) = plan.present else {
    sweep_unclaimed_cameras(&directory, None);
    return;
  };
  let camera_path = camera_for_recording(&path);
  sweep_unclaimed_cameras(&directory, camera_path.as_deref());

  let recorded_at = std::fs::metadata(&path)
    .and_then(|metadata| metadata.modified())
    .map_or_else(
      |_| chrono::Local::now(),
      chrono::DateTime::<chrono::Local>::from,
    );
  let suggested_file_stem = crate::screenshots::capture_file_stem(recorded_at.naive_local());
  if let Err(error) = present_recording(
    app,
    FinalizeInfo {
      camera: camera_path.map(|path| crate::recording::CameraFinalizeInfo {
        duration_ms: 0,
        height: 0,
        path,
        width: 0,
      }),
      has_microphone: false,
      has_system_audio: false,
      duration_ms: 0,
      height: 0,
      path,
      poster: None,
      primary_kind: crate::recording::PrimaryRecordingKind::Screen,
      source_scale_factor: 1.0,
      width: 0,
    },
    suggested_file_stem,
  ) {
    eprintln!("Could not offer back an unsaved recording: {error}");
  }
}

pub fn initialize(app: &AppHandle) {
  if let Some(directory) = load_directory(app) {
    *app
      .state::<ExportState>()
      .directory
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(directory);
  }

  *app
    .state::<ExportState>()
    .screenshot_radius_percent
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner()) = load_screenshot_radius(app);

  sweep_orphaned_recordings(app);
}
