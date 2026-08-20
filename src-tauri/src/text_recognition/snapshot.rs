// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{collections::HashMap, sync::Mutex};

use serde::Serialize;
use tauri::{ipc::Channel, ipc::InvokeResponseBody, State};

use crate::{
  recording::Region,
  screenshots::{self, CapturedImage},
};

struct MonitorSnapshot {
  image: CapturedImage,
  scale: f64,
}

#[derive(Default)]
struct Session {
  generation: u64,
  monitors: HashMap<u32, MonitorSnapshot>,
  selected: Option<CapturedImage>,
}

#[derive(Default)]
pub struct TextRecognitionState(Mutex<Session>);

impl TextRecognitionState {
  pub(super) fn begin(&self) -> u64 {
    let mut session = self
      .0
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    session.generation = session.generation.wrapping_add(1);
    session.monitors.clear();
    session.selected = None;
    session.generation
  }

  pub(super) fn cancel(&self) -> bool {
    let mut session = self
      .0
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    session.generation = session.generation.wrapping_add(1);
    let had_capture = !session.monitors.is_empty() || session.selected.is_some();
    session.monitors.clear();
    session.selected = None;
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
      .map(|(id, scale, image)| (id, MonitorSnapshot { image, scale }))
      .collect();
    true
  }

  pub(super) fn selected(&self) -> Option<CapturedImage> {
    self
      .0
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .selected
      .clone()
  }

  pub(super) fn select_region(
    &self,
    monitor_id: u32,
    region: Region,
  ) -> Result<CapturedImage, String> {
    let mut session = self
      .0
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    let snapshot = session
      .monitors
      .get(&monitor_id)
      .ok_or_else(|| "The frozen monitor image is no longer available".to_owned())?;
    let image = crop(&snapshot.image, snapshot.scale, region)?;
    session.selected = Some(image.clone());
    Ok(image)
  }
}

fn crop(image: &CapturedImage, scale: f64, region: Region) -> Result<CapturedImage, String> {
  let rect = screenshots::physical_capture_rect(region, scale, image.width, image.height)
    .ok_or_else(|| "The selected region is not on the monitor".to_owned())?;
  let source_stride = image.width as usize * 4;
  let target_stride = rect.width as usize * 4;
  let mut rgba = vec![0_u8; target_stride * rect.height as usize];
  for row in 0..rect.height as usize {
    let source_start = (rect.y as usize + row) * source_stride + rect.x as usize * 4;
    let target_start = row * target_stride;
    rgba[target_start..target_start + target_stride]
      .copy_from_slice(&image.rgba[source_start..source_start + target_stride]);
  }
  Ok(CapturedImage {
    height: rect.height,
    rgba,
    width: rect.width,
  })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextRecognitionSnapshot {
  height: u32,
  width: u32,
}

#[tauri::command]
pub fn get_text_recognition_snapshot(
  state: State<'_, TextRecognitionState>,
  monitor_id: u32,
  channel: Channel,
) -> Result<TextRecognitionSnapshot, String> {
  let (pixels, width, height) = {
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
    )
  };
  channel
    .send(InvokeResponseBody::Raw(pixels))
    .map_err(|error| error.to_string())?;
  Ok(TextRecognitionSnapshot { height, width })
}

#[cfg(test)]
mod tests {
  use super::*;
  use tauri::{LogicalPosition, LogicalSize};

  #[test]
  fn crop_uses_logical_selection_at_monitor_scale() {
    let image = CapturedImage {
      height: 4,
      rgba: (0_u8..64).collect(),
      width: 4,
    };
    let cropped = crop(
      &image,
      2.0,
      Region {
        position: LogicalPosition::new(0.5, 0.5),
        size: LogicalSize::new(1.0, 1.0),
      },
    )
    .unwrap();

    assert_eq!((cropped.width, cropped.height), (2, 2));
    assert_eq!(
      cropped.rgba,
      [&image.rgba[20..28], &image.rgba[36..44]].concat()
    );
  }
}
