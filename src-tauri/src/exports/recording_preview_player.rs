// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native, bounded playback for the export window.
//!
//! Rust owns decode, audio output, seeking and the playback clock. Native
//! platform surfaces own video presentation; the webview only supplies layout
//! and interaction state.

use std::{
  path::PathBuf,
  sync::{atomic::AtomicBool, atomic::Ordering, Arc, Mutex, RwLock},
};

use serde::{Deserialize, Serialize};
use tauri::{ipc::Channel, AppHandle, Manager};

mod audio;
pub(crate) mod commands;
mod layout;
mod platform;
#[cfg(target_os = "windows")]
pub(crate) use platform::GpuVideoReader;
pub(crate) mod surface_commands;
pub(crate) mod timeline_thumbnails;
mod video;
mod worker;

use self::layout::{preview_layout, RecordingPreviewLayout};
use self::worker::{PlaybackMode, PreviewPlayerWorker};
use super::preview_platform::workspace_editor::{
  apply_layer_gesture, fit_canvas_to_layers, GestureOperation as WorkspaceGestureOperation,
  LayerGeometry, NormalizedRect, WorkspaceScene,
};
use super::preview_platform::RecordingPreviewSurface;
use super::preview_platform::{SelectionGestureOperation, SelectionGesturePhase};
use super::{
  cursor_effects::{CursorCompositor, CursorEffectSettings},
  AudioTrackVolume, CameraOverlaySettings, ExportArtifact, ExportState, RecordingAudioTrack,
  RecordingOutputSettings,
};
use crate::recording::PrimaryRecordingKind;
pub use commands::stop_all;

pub(super) const AUTO_FIT_MOVE_EDGE: u32 = 1 << 17;
const AUTO_FIT_COMMIT_EDGE: u32 = 1 << 18;

#[derive(Clone)]
pub(super) struct PlayerSources {
  audio_tracks: Vec<RecordingAudioTrack>,
  camera_duration_ms: Option<u64>,
  camera_path: Option<PathBuf>,
  #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
  cursor: Option<Arc<CursorCompositor>>,
  cursor_settings: Arc<RwLock<CursorEffectSettings>>,
  composition_settings: Option<Arc<RwLock<PreviewCompositionSettings>>>,
  duration_ms: u64,
  /// Zero when OSCs are hidden, one for the primary pane and two for camera.
  layout: RecordingPreviewLayout,
  playback_layout: RecordingPreviewLayout,
  /// True while real-time playback owns the surface, so a late still decode
  /// never stomps a playing frame.
  playing: Arc<AtomicBool>,
  preview_surface: Option<Arc<RecordingPreviewSurface>>,
  screen_path: PathBuf,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewAudioSettings {
  pub audio_track_volumes: Vec<AudioTrackVolume>,
  pub enabled_stream_indices: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewPlayerSettings {
  pub audio: PreviewAudioSettings,
  pub bake_camera: bool,
  pub camera_overlay: CameraOverlaySettings,
  pub cursor_effects: CursorEffectSettings,
  pub recording_output: RecordingOutputSettings,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
struct PreviewCompositionSettings {
  bake_camera: bool,
  camera_overlay: CameraOverlaySettings,
  recording_output: RecordingOutputSettings,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
struct RecordingSelectionGesture {
  snapshot: PreviewCompositionSettings,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecordingPreviewPlayerInfo {
  pub duration_ms: u64,
  pub layout: RecordingPreviewLayout,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecordingPreviewTransformEvent {
  session_id: u64,
  zoom_percent: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(
  rename_all = "camelCase",
  rename_all_fields = "camelCase",
  tag = "event",
  content = "data"
)]
pub enum RecordingPreviewPlayerEvent {
  Ended,
  Error { message: String },
  Paused { position_ms: u64 },
  Playing { position_ms: u64 },
  Position { position_ms: u64 },
  Ready { position_ms: u64, request_id: u64 },
}

#[derive(Default)]
struct PreviewPlayerManager {
  artifact_id: Option<u64>,
  audio_indices: Vec<usize>,
  audio_volumes: Vec<AudioTrackVolume>,
  event_channel: Option<Channel<RecordingPreviewPlayerEvent>>,
  frame_channel: Option<Channel>,
  is_playing: bool,
  latest_session_id: u64,
  latest_layout_request: u64,
  latest_seek_request: u64,
  pane_target_sizes: Vec<(u32, u32)>,
  position_ms: u64,
  /// The next still seek came from a scrub gesture in progress, so the
  /// scrubber may land on the cheapest nearby frame for immediacy.
  rough_seek: bool,
  #[cfg(any(target_os = "macos", target_os = "windows"))]
  selection_gesture: Option<RecordingSelectionGesture>,
  sources: Option<PlayerSources>,
  session_id: Option<u64>,
  still_decoder: Option<platform::StillDecoder>,
  workspace_scene: Option<WorkspaceScene>,
  worker: Option<PreviewPlayerWorker>,
}

impl PreviewPlayerManager {
  fn selection_composition(&self) -> Option<PreviewCompositionSettings> {
    self
      .sources
      .as_ref()?
      .composition_settings
      .as_ref()?
      .read()
      .ok()
      .map(|settings| settings.clone())
  }

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
  fn take_worker(&mut self) -> Option<PreviewPlayerWorker> {
    let worker = self.worker.take()?;
    worker.signal_cancel();
    self.position_ms = worker.position();
    Some(worker)
  }

  fn restart(&mut self, mode: PlaybackMode) -> Result<(), String> {
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
    let frame_channel = self
      .frame_channel
      .clone()
      .ok_or_else(|| "The recording preview frame channel is unavailable".to_owned())?;
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
        self.still_decoder = Some(platform::StillDecoder::spawn(
          sources,
          event_channel,
          frame_channel,
        )?);
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
      frame_channel,
      event_channel,
    )?);
    Ok(())
  }

  /// Recomposes the paused stills from the cached full-resolution sources
  /// against whatever composition is current, the way macOS recomposes its
  /// retained workspace. `Ok(false)` means a source is not cached yet and the
  /// decoder has to supply the frame - `redraw_still` still flushes its
  /// present batch on that path, so geometry a deferred layout parked reaches
  /// the compositor instead of stranding the pane at its previous box.
  #[cfg(target_os = "windows")]
  fn redraw_still_now(&self) -> Result<bool, String> {
    let sources = self
      .sources
      .as_ref()
      .ok_or_else(|| "The recording preview player is not open".to_owned())?;
    let surface = sources
      .preview_surface
      .as_ref()
      .ok_or_else(|| "The recording preview surface is unavailable".to_owned())?;
    let composition = sources
      .composition_settings
      .as_ref()
      .ok_or_else(|| "The recording preview composition is unavailable".to_owned())?
      .read()
      .map_err(|_| "The recording preview composition is unavailable".to_owned())?
      .clone();
    surface.redraw_still(
      composition.bake_camera && sources.camera_path.is_some(),
      &composition.recording_output.primary,
      &composition.recording_output.camera,
      composition.camera_overlay,
      composition.recording_output.camera.drop_shadow,
      composition.recording_output.camera_on_top,
    )
  }

  #[cfg(target_os = "windows")]
  fn refresh_selection_preview(&self, _layer_id: u32) -> Result<(), String> {
    self.redraw_still_now().map(|_| ())
  }

  #[cfg(target_os = "macos")]
  fn refresh_selection_preview(&mut self, layer_id: u32) -> Result<(), String> {
    let retained = self
      .sources
      .as_ref()
      .and_then(|sources| {
        let surface = sources.preview_surface.as_ref()?;
        let composition = sources.composition_settings.as_ref()?.read().ok()?.clone();
        if layer_id != 1 {
          return None;
        }
        let mut panes = vec![(0, &composition.recording_output.primary)];
        if !composition.bake_camera && sources.camera_path.is_some() {
          panes.push((1, &composition.recording_output.camera));
        }
        surface
          .recompose_recording_workspace(
            &panes,
            composition.bake_camera.then_some((
              composition.camera_overlay,
              composition.recording_output.camera.drop_shadow,
              composition.recording_output.camera_on_top,
            )),
          )
          .ok()
          .filter(|updated| *updated)
          .map(|_| surface.redraw_recording_workspace())
      })
      .unwrap_or(false);
    if retained {
      return Ok(());
    }
    self.restart(PlaybackMode::InteractiveStill)
  }

  #[cfg(any(target_os = "macos", target_os = "windows"))]
  fn handle_selection_gesture(
    &mut self,
    phase: SelectionGesturePhase,
    layer_id: u32,
    operation: SelectionGestureOperation,
    edges: u32,
    scale: f64,
    delta_x: f64,
    delta_y: f64,
  ) -> Result<(), String> {
    let settings = self
      .sources
      .as_ref()
      .and_then(|sources| sources.composition_settings.clone())
      .ok_or_else(|| "The recording preview composition is unavailable".to_owned())?;
    match phase {
      SelectionGesturePhase::Begin => {
        let snapshot = settings
          .read()
          .map_err(|_| "The recording preview composition is unavailable".to_owned())?
          .clone();
        // Crop display composition is controlled by React's uncropped
        // preview output. Do not freeze the old selected layer here: the
        // selection OSC changes synchronously on mouse-down, and React must
        // be able to present the newly selected layer's uncropped pixels
        // before the first crop update arrives.
        if matches!(
          operation,
          SelectionGestureOperation::CropMove | SelectionGestureOperation::CropResize
        ) {
          self.selection_gesture = None;
          return Ok(());
        }
        self.selection_gesture = Some(RecordingSelectionGesture { snapshot });
        Ok(())
      }
      SelectionGesturePhase::Update | SelectionGesturePhase::End => {
        if matches!(
          operation,
          SelectionGestureOperation::CropMove | SelectionGestureOperation::CropResize
        ) {
          // Crop pixels are mirrored by React's uncropped composition. Keep
          // this native manager out of the gesture snapshot so each selected
          // layer can present immediately during the crop interaction.
          return Ok(());
        }
        let ending = matches!(phase, SelectionGesturePhase::End);
        if self.selection_gesture.is_none() {
          let snapshot = settings
            .read()
            .map_err(|_| "The recording preview composition is unavailable".to_owned())?
            .clone();
          self.selection_gesture = Some(RecordingSelectionGesture { snapshot });
        }
        if operation == SelectionGestureOperation::Move && edges & AUTO_FIT_COMMIT_EDGE != 0 {
          let current = settings
            .read()
            .map_err(|_| "The recording preview composition is unavailable".to_owned())?
            .clone();
          if let Some(gesture) = self.selection_gesture.as_mut() {
            gesture.snapshot = current;
          }
          return Ok(());
        }
        let Some(gesture) = self.selection_gesture.as_ref() else {
          return Ok(());
        };
        let snapshot = &gesture.snapshot;
        let mut next = snapshot.clone();
        if matches!(
          operation,
          SelectionGestureOperation::FrameResize | SelectionGestureOperation::FrameRadius
        ) {
          if operation == SelectionGestureOperation::FrameRadius {
            return Ok(());
          }
          let Some(mut scene) = self.workspace_scene.clone() else {
            return Ok(());
          };
          let output = match layer_id {
            0 => &snapshot.recording_output.primary,
            1 if !snapshot.bake_camera => &snapshot.recording_output.camera,
            _ => return Ok(()),
          };
          let Some(frame) = scene.frames.iter_mut().find(|frame| frame.id.0 == layer_id) else {
            return Ok(());
          };
          frame.rect.width = f64::from(output.width);
          frame.rect.height = f64::from(output.height);
          let (scene, recording_output, camera_overlay) =
            super::preview_workspace_model::resize_recording_frame(
              &scene,
              &snapshot.recording_output,
              snapshot.camera_overlay,
              snapshot.bake_camera,
              layer_id,
              edges,
              (delta_x, delta_y),
            )?;
          next.recording_output = recording_output;
          next.camera_overlay = camera_overlay;
          self.workspace_scene = Some(scene);
          *settings
            .write()
            .map_err(|_| "The recording preview composition is unavailable".to_owned())? = next;
          // The retained native workspace updates media and OSC in the same
          // command buffer. React mirrors this state, and the final layout
          // performs the one post-gesture decoder reconciliation.
          if ending {
            self.selection_gesture = None;
          }
          return Ok(());
        }
        if snapshot.bake_camera && layer_id == 1 {
          let start = snapshot.camera_overlay;
          let operation = match operation {
            SelectionGestureOperation::Move => WorkspaceGestureOperation::Move,
            SelectionGestureOperation::Resize => WorkspaceGestureOperation::Resize,
            SelectionGestureOperation::Radius => WorkspaceGestureOperation::Radius,
            SelectionGestureOperation::FrameResize | SelectionGestureOperation::FrameRadius => {
              return Ok(())
            }
            SelectionGestureOperation::CropMove | SelectionGestureOperation::CropResize => {
              unreachable!("crop gestures are mirrored by the frontend")
            }
          };
          let mut geometry = apply_layer_gesture(
            LayerGeometry {
              crop: NormalizedRect {
                x: start.frame_x_percent / 100.0,
                y: start.frame_y_percent / 100.0,
                width: start.frame_width_percent / 100.0,
                height: start.frame_height_percent / 100.0,
              },
              image_center_x: start.camera_x_percent / 100.0,
              image_center_y: start.camera_y_percent / 100.0,
              image_width: start.camera_width_percent / 100.0,
              radius_percent: start.radius_percent,
            },
            operation,
            (delta_x, delta_y),
            scale,
          );
          if operation == WorkspaceGestureOperation::Move && edges & AUTO_FIT_MOVE_EDGE != 0 {
            let primary = &snapshot.recording_output.primary;
            let primary_geometry = LayerGeometry {
              crop: NormalizedRect {
                x: primary.screenshot_crop_x_percent / 100.0,
                y: primary.screenshot_crop_y_percent / 100.0,
                width: primary.screenshot_crop_width_percent / 100.0,
                height: primary.screenshot_crop_height_percent / 100.0,
              },
              image_center_x: primary.screenshot_image_x_percent / 100.0,
              image_center_y: primary.screenshot_image_y_percent / 100.0,
              image_width: primary.screenshot_image_width_percent / 100.0,
              radius_percent: primary.radius_percent,
            };
            let ((width, height), fitted) = fit_canvas_to_layers(
              (primary.width, primary.height),
              &[primary_geometry, geometry],
            );
            let fitted_primary = fitted[0];
            geometry = fitted[1];
            next.recording_output.primary.width = width;
            next.recording_output.primary.height = height;
            next.recording_output.primary.screenshot_crop_x_percent = fitted_primary.crop.x * 100.0;
            next.recording_output.primary.screenshot_crop_y_percent = fitted_primary.crop.y * 100.0;
            next.recording_output.primary.screenshot_crop_width_percent =
              fitted_primary.crop.width * 100.0;
            next.recording_output.primary.screenshot_crop_height_percent =
              fitted_primary.crop.height * 100.0;
            next.recording_output.primary.screenshot_image_x_percent =
              fitted_primary.image_center_x * 100.0;
            next.recording_output.primary.screenshot_image_y_percent =
              fitted_primary.image_center_y * 100.0;
            next.recording_output.primary.screenshot_image_width_percent =
              fitted_primary.image_width * 100.0;
          }
          next.camera_overlay.frame_x_percent = geometry.crop.x * 100.0;
          next.camera_overlay.frame_y_percent = geometry.crop.y * 100.0;
          next.camera_overlay.frame_width_percent = geometry.crop.width * 100.0;
          next.camera_overlay.frame_height_percent = geometry.crop.height * 100.0;
          next.camera_overlay.camera_x_percent = geometry.image_center_x * 100.0;
          next.camera_overlay.camera_y_percent = geometry.image_center_y * 100.0;
          next.camera_overlay.camera_width_percent = geometry.image_width * 100.0;
          next.camera_overlay.radius_percent = geometry.radius_percent;
          *settings
            .write()
            .map_err(|_| "The recording preview composition is unavailable".to_owned())? = next;
          if edges & AUTO_FIT_MOVE_EDGE != 0 {
            if ending {
              self.selection_gesture = None;
            }
            return Ok(());
          }
          let result = self.refresh_selection_preview(layer_id);
          if ending {
            self.selection_gesture = None;
          }
          return result;
        }
        let (start, output) = match layer_id {
          0 => (
            &snapshot.recording_output.primary,
            &mut next.recording_output.primary,
          ),
          1 => (
            &snapshot.recording_output.camera,
            &mut next.recording_output.camera,
          ),
          _ => return Ok(()),
        };
        let operation = match operation {
          SelectionGestureOperation::Move => WorkspaceGestureOperation::Move,
          SelectionGestureOperation::Resize => WorkspaceGestureOperation::Resize,
          SelectionGestureOperation::Radius => WorkspaceGestureOperation::Radius,
          SelectionGestureOperation::FrameResize | SelectionGestureOperation::FrameRadius => {
            return Ok(())
          }
          SelectionGestureOperation::CropMove | SelectionGestureOperation::CropResize => {
            unreachable!("crop gestures are mirrored by the frontend")
          }
        };
        let mut geometry = apply_layer_gesture(
          LayerGeometry {
            crop: NormalizedRect {
              x: start.screenshot_crop_x_percent / 100.0,
              y: start.screenshot_crop_y_percent / 100.0,
              width: start.screenshot_crop_width_percent / 100.0,
              height: start.screenshot_crop_height_percent / 100.0,
            },
            image_center_x: start.screenshot_image_x_percent / 100.0,
            image_center_y: start.screenshot_image_y_percent / 100.0,
            image_width: start.screenshot_image_width_percent / 100.0,
            radius_percent: start.radius_percent,
          },
          operation,
          (delta_x, delta_y),
          scale,
        );
        if operation == WorkspaceGestureOperation::Move && edges & AUTO_FIT_MOVE_EDGE != 0 {
          let ((width, height), fitted) =
            fit_canvas_to_layers((start.width, start.height), &[geometry]);
          geometry = fitted[0];
          output.width = width;
          output.height = height;
        }
        output.screenshot_crop_x_percent = geometry.crop.x * 100.0;
        output.screenshot_crop_y_percent = geometry.crop.y * 100.0;
        output.screenshot_crop_width_percent = geometry.crop.width * 100.0;
        output.screenshot_crop_height_percent = geometry.crop.height * 100.0;
        output.screenshot_image_x_percent = geometry.image_center_x * 100.0;
        output.screenshot_image_y_percent = geometry.image_center_y * 100.0;
        output.screenshot_image_width_percent = geometry.image_width * 100.0;
        output.radius_percent = geometry.radius_percent;
        *settings
          .write()
          .map_err(|_| "The recording preview composition is unavailable".to_owned())? = next;
        if edges & AUTO_FIT_MOVE_EDGE != 0 {
          if ending {
            self.selection_gesture = None;
          }
          return Ok(());
        }
        let result = self.refresh_selection_preview(layer_id);
        if ending {
          self.selection_gesture = None;
        }
        result
      }
      SelectionGesturePhase::Cancel => {
        let Some(gesture) = self.selection_gesture.take() else {
          return Ok(());
        };
        *settings
          .write()
          .map_err(|_| "The recording preview composition is unavailable".to_owned())? =
          gesture.snapshot;
        self.restart(PlaybackMode::InteractiveStill)
      }
    }
  }

  /// How much each pane's playback decode shrinks to match the on-screen pane
  /// size, mirroring what the still decoder presents.
  fn playback_factors(&self, sources: &PlayerSources) -> Vec<f64> {
    platform::playback_factors(&self.pane_target_sizes, sources)
  }

  fn stop(&mut self) {
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
    self.frame_channel = None;
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

  fn require_session(&self, session_id: u64) -> Result<(), String> {
    (self.session_id == Some(session_id))
      .then_some(())
      .ok_or_else(|| "That recording preview player session is no longer active".to_owned())
  }
}

#[derive(Default)]
pub struct RecordingPreviewPlayerState(Mutex<PreviewPlayerManager>);

fn sources(
  app: &AppHandle,
  artifact_id: u64,
  settings: Option<&PreviewPlayerSettings>,
) -> Result<PlayerSources, String> {
  let state = app.state::<ExportState>();
  let (audio_tracks, camera, cursor_path, duration_ms, height, path, primary_kind, width) = {
    let artifact = state
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
    .get_webview_window("export")
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
