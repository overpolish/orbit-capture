// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use serde::Serialize;

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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn lays_out_a_screen_as_one_native_preview_pane() {
    let layout = preview_layout(Some((3_600, 2_338, PreviewPaneKind::Screen)), None, 720);

    assert_eq!(layout.panes.len(), 1);
    assert!(matches!(layout.panes[0].kind, PreviewPaneKind::Screen));
    assert_eq!(layout.panes[0].x, 0);
    assert_eq!(layout.width, layout.panes[0].width);
  }

  #[test]
  fn keeps_screen_and_portrait_camera_as_separate_panes() {
    let layout = preview_layout(
      Some((3_600, 2_338, PreviewPaneKind::Screen)),
      Some((1_080, 1_920)),
      720,
    );

    assert_eq!(layout.panes.len(), 2);
    assert!(matches!(layout.panes[0].kind, PreviewPaneKind::Screen));
    assert!(matches!(layout.panes[1].kind, PreviewPaneKind::Camera));
    assert_eq!(layout.panes[1].x, layout.panes[0].width);
    assert_eq!(layout.width, layout.panes[0].width + layout.panes[1].width);
    assert!(layout.panes[1].width < layout.panes[0].width);
  }

  #[test]
  fn lays_out_a_primary_camera_as_a_camera_pane() {
    let layout = preview_layout(Some((1_920, 1_080, PreviewPaneKind::Camera)), None, 720);

    assert_eq!(layout.panes.len(), 1);
    assert!(matches!(layout.panes[0].kind, PreviewPaneKind::Camera));
  }
}
