// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use serde::Serialize;

pub(super) const PREVIEW_HEIGHT: u32 = 720;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PreviewPaneKind {
  Camera,
  Screen,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewPane {
  pub height: u32,
  pub kind: PreviewPaneKind,
  pub source_height: u32,
  pub source_width: u32,
  pub width: u32,
  pub x: u32,
  pub y: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingPreviewLayout {
  pub height: u32,
  pub panes: Vec<PreviewPane>,
  pub width: u32,
}

fn scaled_width(width: u32, height: u32, target_height: u32) -> u32 {
  if width == 0 || height == 0 {
    return 0;
  }
  let scaled = f64::from(width) * f64::from(target_height) / f64::from(height);
  ((scaled / 2.0).round().max(1.0) as u32) * 2
}

pub(super) fn preview_layout(
  primary: Option<(u32, u32, PreviewPaneKind)>,
  camera: Option<(u32, u32)>,
  height: u32,
) -> RecordingPreviewLayout {
  let mut panes =
    Vec::with_capacity(usize::from(primary.is_some()) + usize::from(camera.is_some()));
  let primary_width = primary.map_or(0, |primary| scaled_width(primary.0, primary.1, height));
  if let Some((source_width, source_height, kind)) = primary.filter(|_| primary_width > 0) {
    panes.push(PreviewPane {
      height,
      kind,
      source_height,
      source_width,
      width: primary_width,
      x: 0,
      y: 0,
    });
  }
  if let Some(camera) = camera {
    let width = scaled_width(camera.0, camera.1, height);
    panes.push(PreviewPane {
      height,
      kind: PreviewPaneKind::Camera,
      source_height: camera.1,
      source_width: camera.0,
      width,
      x: primary_width,
      y: 0,
    });
  }
  RecordingPreviewLayout {
    height,
    width: panes.iter().map(|pane| pane.width).sum(),
    panes,
  }
}
