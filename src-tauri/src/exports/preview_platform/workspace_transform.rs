// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Platform-neutral geometry for treating every preview pane as one workspace.

use super::PreviewSurfaceRect;

#[derive(Clone, Copy)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(super) struct WorkspaceTransform {
  pub pan_x: f64,
  pub pan_y: f64,
  pub zoom: f64,
}

impl Default for WorkspaceTransform {
  fn default() -> Self {
    Self {
      pan_x: 0.0,
      pan_y: 0.0,
      zoom: 1.0,
    }
  }
}

impl WorkspaceTransform {
  #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
  pub(super) fn apply(
    self,
    viewport: PreviewSurfaceRect,
    pane: PreviewSurfaceRect,
  ) -> PreviewSurfaceRect {
    let viewport_center_x = viewport.width / 2.0;
    let viewport_center_y = viewport.height / 2.0;
    let pane_center_x = pane.x + pane.width / 2.0;
    let pane_center_y = pane.y + pane.height / 2.0;
    let width = pane.width * self.zoom;
    let height = pane.height * self.zoom;
    let center_x = viewport_center_x + self.pan_x + (pane_center_x - viewport_center_x) * self.zoom;
    let center_y = viewport_center_y + self.pan_y + (pane_center_y - viewport_center_y) * self.zoom;
    PreviewSurfaceRect {
      height,
      width,
      x: center_x - width / 2.0,
      y: center_y - height / 2.0,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn rect(x: f64, y: f64, width: f64, height: f64) -> PreviewSurfaceRect {
    PreviewSurfaceRect {
      height,
      width,
      x,
      y,
    }
  }

  #[test]
  fn multiple_panes_keep_their_relative_workspace_geometry() {
    let viewport = rect(100.0, 50.0, 1_000.0, 600.0);
    let transform = WorkspaceTransform {
      pan_x: 25.0,
      pan_y: -10.0,
      zoom: 2.0,
    };
    let screen = transform.apply(viewport, rect(100.0, 150.0, 300.0, 200.0));
    let camera = transform.apply(viewport, rect(424.0, 175.0, 200.0, 150.0));

    assert_eq!(
      (screen.x, screen.y, screen.width, screen.height),
      (-275.0, -10.0, 600.0, 400.0)
    );
    assert_eq!(
      (camera.x, camera.y, camera.width, camera.height),
      (373.0, 40.0, 400.0, 300.0)
    );
    assert_eq!(camera.x - (screen.x + screen.width), 48.0);
  }

  #[test]
  fn identity_leaves_each_pane_unchanged() {
    let pane = rect(27.0, 31.0, 640.0, 360.0);
    let transformed = WorkspaceTransform::default().apply(rect(0.0, 0.0, 900.0, 600.0), pane);
    assert_eq!(
      (
        transformed.x,
        transformed.y,
        transformed.width,
        transformed.height
      ),
      (pane.x, pane.y, pane.width, pane.height)
    );
  }
}
