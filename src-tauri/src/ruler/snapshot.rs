// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
  collections::HashMap,
  sync::{Arc, Mutex},
};

use serde::Serialize;
use tauri::{ipc::Channel, ipc::InvokeResponseBody, State};

use super::analysis::{compute_gradients, detect_boxes, ComponentBox, GradientMaps};
use crate::screenshots::CapturedImage;

struct MonitorSnapshot {
  image: CapturedImage,
  scale: f64,
  /// Derived lazily on first request and dropped together with the snapshot it
  /// was computed from, so a new freeze can never serve stale gradients.
  gradients: Option<Arc<GradientMaps>>,
}

#[derive(Default)]
struct Session {
  generation: u64,
  monitors: HashMap<u32, MonitorSnapshot>,
}

#[derive(Default)]
pub struct RulerState(Mutex<Session>);

impl RulerState {
  pub(super) fn begin(&self) -> u64 {
    let mut session = self
      .0
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    session.generation = session.generation.wrapping_add(1);
    session.monitors.clear();
    session.generation
  }

  pub(super) fn cancel(&self) -> bool {
    let mut session = self
      .0
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    session.generation = session.generation.wrapping_add(1);
    let had_capture = !session.monitors.is_empty();
    session.monitors.clear();
    had_capture
  }

  pub(super) fn install(
    &self,
    generation: u64,
    monitors: impl IntoIterator<Item = (u32, f64, CapturedImage)>,
  ) -> bool {
    let mut session = self
      .0
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    if session.generation != generation {
      return false;
    }
    session.monitors = monitors
      .into_iter()
      .map(|(id, scale, image)| {
        (
          id,
          MonitorSnapshot {
            image,
            scale,
            gradients: None,
          },
        )
      })
      .collect();
    true
  }

  fn session(&self) -> std::sync::MutexGuard<'_, Session> {
    self
      .0
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
  }

  /// The already derived gradients, or a copy of the frozen pixels plus the
  /// generation they belong to so the result can be discarded if the session
  /// moved on while the maps were being computed.
  fn gradient_source(&self, monitor_id: u32) -> Option<GradientSource> {
    let session = self.session();
    let snapshot = session.monitors.get(&monitor_id)?;
    Some(match &snapshot.gradients {
      Some(gradients) => GradientSource::Ready(Arc::clone(gradients)),
      None => GradientSource::Pixels {
        generation: session.generation,
        rgba: snapshot.image.rgba.clone(),
        width: snapshot.image.width,
        height: snapshot.image.height,
      },
    })
  }

  fn store_gradients(&self, generation: u64, monitor_id: u32, gradients: &Arc<GradientMaps>) {
    let mut session = self.session();
    if session.generation != generation {
      return;
    }
    if let Some(snapshot) = session.monitors.get_mut(&monitor_id) {
      snapshot.gradients = Some(Arc::clone(gradients));
    }
  }
}

enum GradientSource {
  Ready(Arc<GradientMaps>),
  Pixels {
    generation: u64,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
  },
}

const MISSING: &str = "The frozen monitor image is no longer available";

/// Derives (or reuses) the gradient maps for one monitor. The heavy pass runs on
/// a blocking worker with the session mutex released: a 5K monitor is ~15M
/// pixels, and holding the lock would stall every other ruler command.
async fn gradients_for(
  state: &State<'_, RulerState>,
  monitor_id: u32,
) -> Result<Arc<GradientMaps>, String> {
  let (generation, rgba, width, height) = match state
    .gradient_source(monitor_id)
    .ok_or_else(|| MISSING.to_owned())?
  {
    GradientSource::Ready(gradients) => return Ok(gradients),
    GradientSource::Pixels {
      generation,
      rgba,
      width,
      height,
    } => (generation, rgba, width, height),
  };
  let gradients =
    tauri::async_runtime::spawn_blocking(move || Arc::new(compute_gradients(&rgba, width, height)))
      .await
      .map_err(|error| error.to_string())?;
  state.store_gradients(generation, monitor_id, &gradients);
  Ok(gradients)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RulerSnapshot {
  height: u32,
  scale: f64,
  width: u32,
}

#[tauri::command]
pub fn get_ruler_snapshot(
  state: State<'_, RulerState>,
  monitor_id: u32,
  channel: Channel,
) -> Result<RulerSnapshot, String> {
  let (pixels, width, height, scale) = {
    let session = state
      .0
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    let snapshot = session
      .monitors
      .get(&monitor_id)
      .ok_or_else(|| "The frozen monitor image is no longer available".to_owned())?;
    (
      snapshot.image.rgba.clone(),
      snapshot.image.width,
      snapshot.image.height,
      snapshot.scale,
    )
  };
  channel
    .send(InvokeResponseBody::Raw(pixels))
    .map_err(|error| error.to_string())?;
  Ok(RulerSnapshot {
    height,
    scale,
    width,
  })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RulerGradientsMeta {
  height: u32,
  width: u32,
}

/// Streams the horizontal gradient plane followed by the vertical one as a
/// single raw buffer; each plane is exactly `width * height` bytes.
#[tauri::command]
pub async fn get_ruler_gradients(
  state: State<'_, RulerState>,
  monitor_id: u32,
  channel: Channel,
) -> Result<RulerGradientsMeta, String> {
  let gradients = gradients_for(&state, monitor_id).await?;
  let mut planes = Vec::with_capacity(gradients.gx.len() + gradients.gy.len());
  planes.extend_from_slice(&gradients.gx);
  planes.extend_from_slice(&gradients.gy);
  channel
    .send(InvokeResponseBody::Raw(planes))
    .map_err(|error| error.to_string())?;
  Ok(RulerGradientsMeta {
    height: gradients.height,
    width: gradients.width,
  })
}

#[tauri::command]
pub async fn get_ruler_boxes(
  state: State<'_, RulerState>,
  monitor_id: u32,
  threshold: u8,
) -> Result<Vec<ComponentBox>, String> {
  let gradients = gradients_for(&state, monitor_id).await?;
  tauri::async_runtime::spawn_blocking(move || detect_boxes(&gradients, threshold))
    .await
    .map_err(|error| error.to_string())
}
