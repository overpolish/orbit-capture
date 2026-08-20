// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[derive(Clone)]
pub(super) struct PlayerSources {
  pub(super) audio_tracks: Vec<RecordingAudioTrack>,
  pub(super) camera_duration_ms: Option<u64>,
  pub(super) camera_path: Option<PathBuf>,
  #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
  pub(super) cursor: Option<Arc<CursorCompositor>>,
  pub(super) cursor_settings: Arc<RwLock<CursorEffectSettings>>,
  pub(super) composition_settings: Option<Arc<RwLock<PreviewCompositionSettings>>>,
  pub(super) duration_ms: u64,
  /// Zero when OSCs are hidden, one for the primary pane and two for camera.
  pub(super) layout: RecordingPreviewLayout,
  pub(super) playback_layout: RecordingPreviewLayout,
  /// True while real-time playback owns the surface, so a late still decode
  /// never stomps a playing frame.
  pub(super) playing: Arc<AtomicBool>,
  pub(super) preview_surface: Option<Arc<RecordingPreviewSurface>>,
  pub(super) screen_path: PathBuf,
}

pub(super) fn sources(
  app: &AppHandle,
  artifact_id: u64,
  settings: Option<&PreviewPlayerSettings>,
) -> Result<PlayerSources, String> {
  let state = app.state::<ExportState>();
  let (audio_tracks, camera, cursor_path, duration_ms, height, path, primary_kind, width) = {
    let artifact = state
      .recording
      .artifact
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(ExportArtifact::Recording {
      audio_tracks,
      camera,
      cursor,
      duration_ms,
      height,
      id,
      path,
      primary_kind,
      width,
      ..
    }) = artifact.as_ref()
    else {
      return Err("There is no recording to preview".to_owned());
    };
    if *id != artifact_id {
      return Err("That recording is no longer waiting to be exported".to_owned());
    }
    (
      audio_tracks.clone(),
      camera.clone(),
      cursor.as_ref().map(|value| value.path.clone()),
      *duration_ms,
      *height,
      path.clone(),
      *primary_kind,
      *width,
    )
  };
  let camera_size = camera.as_ref().map(|value| (value.width, value.height));
  let primary_pane = match primary_kind {
    PrimaryRecordingKind::Screen => Some((width, height, layout::PreviewPaneKind::Screen)),
    PrimaryRecordingKind::Camera => Some((width, height, layout::PreviewPaneKind::Camera)),
    PrimaryRecordingKind::Audio => None,
  };
  // Creating the native surface synchronously asks the main thread for the
  // export window's NSView/HWND. Never do that while holding the artifact
  // mutex: the main thread may simultaneously be serving a snapshot request
  // that needs the same mutex, which deadlocks crash recovery on startup.
  let preview_surface = app
    .get_webview_window(ExportKind::Recording.window_label().as_str())
    .map(|window| RecordingPreviewSurface::from_window(&window).map(Arc::new))
    .transpose()?;
  let layout = preview_layout(primary_pane, camera_size, height);
  // Native playback decodes every source at its own stored resolution. The
  // presentation surface handles its visual size, so a portrait camera is not
  // needlessly enlarged to the screen track's height before composition.
  let mut playback_layout = layout.clone();
  for pane in &mut playback_layout.panes {
    pane.width = pane.source_width;
    pane.height = pane.source_height;
  }
  playback_layout.width = playback_layout.panes.iter().map(|pane| pane.width).sum();
  playback_layout.height = playback_layout
    .panes
    .iter()
    .map(|pane| pane.height)
    .max()
    .unwrap_or(0);
  Ok(PlayerSources {
    audio_tracks,
    camera_duration_ms: camera.as_ref().map(|value| value.duration_ms),
    camera_path: camera.as_ref().map(|value| value.path.clone()),
    cursor: cursor_path
      .as_ref()
      .map(|path| CursorCompositor::open(path).map(Arc::new))
      .transpose()?,
    composition_settings: settings.map(|settings| {
      Arc::new(RwLock::new(PreviewCompositionSettings {
        bake_camera: settings.bake_camera,
        camera_overlay: settings.camera_overlay,
        recording_output: settings.recording_output.clone(),
      }))
    }),
    cursor_settings: Arc::new(RwLock::new(
      settings.map_or_else(CursorEffectSettings::default, |settings| {
        settings.cursor_effects
      }),
    )),
    duration_ms,
    layout,
    playback_layout,
    playing: Arc::new(AtomicBool::new(false)),
    preview_surface,
    screen_path: path,
  })
}
