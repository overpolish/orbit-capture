// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashSet;

use super::{
  frame_resize::{resize_frame, FrameResizeResult},
  NormalizedRect, WorldRect,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FrameId(pub u32);
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LayerId(pub u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceKind {
  Screenshot,
  BakedVideo,
  SplitVideo,
}

impl WorkspaceKind {
  pub fn is_video(self) -> bool {
    !matches!(self, Self::Screenshot)
  }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkspaceFrame {
  pub id: FrameId,
  pub rect: WorldRect,
  pub radius_percent: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkspaceLayer {
  pub id: LayerId,
  pub frame_id: FrameId,
  pub rect: NormalizedRect,
  pub radius_percent: f64,
  pub z_index: i32,
}

#[derive(Clone, Debug)]
pub struct WorkspaceScene {
  pub kind: WorkspaceKind,
  pub viewport: WorldRect,
  pub frames: Vec<WorkspaceFrame>,
  pub layers: Vec<WorkspaceLayer>,
  pub revision: u64,
}

impl WorkspaceScene {
  pub fn one_frame(
    kind: WorkspaceKind,
    viewport: WorldRect,
    frame: WorkspaceFrame,
    layers: Vec<WorkspaceLayer>,
  ) -> Result<Self, String> {
    Self::new(kind, viewport, vec![frame], layers)
  }

  pub fn screenshot(
    viewport: WorldRect,
    frame: WorldRect,
    layers: Vec<WorkspaceLayer>,
  ) -> Result<Self, String> {
    Self::one_frame(
      WorkspaceKind::Screenshot,
      viewport,
      WorkspaceFrame {
        id: FrameId(0),
        rect: frame,
        radius_percent: 0.0,
      },
      layers,
    )
  }

  pub fn baked_video(
    viewport: WorldRect,
    frame: WorldRect,
    layers: Vec<WorkspaceLayer>,
  ) -> Result<Self, String> {
    Self::one_frame(
      WorkspaceKind::BakedVideo,
      viewport,
      WorkspaceFrame {
        id: FrameId(0),
        rect: frame,
        radius_percent: 0.0,
      },
      layers,
    )
  }

  pub fn split_video(
    viewport: WorldRect,
    frames: Vec<WorkspaceFrame>,
    layers: Vec<WorkspaceLayer>,
  ) -> Result<Self, String> {
    Self::new(WorkspaceKind::SplitVideo, viewport, frames, layers)
  }

  pub fn new(
    kind: WorkspaceKind,
    viewport: WorldRect,
    frames: Vec<WorkspaceFrame>,
    layers: Vec<WorkspaceLayer>,
  ) -> Result<Self, String> {
    let scene = Self {
      kind,
      viewport,
      frames,
      layers,
      revision: 0,
    };
    scene.validate()?;
    Ok(scene)
  }

  pub fn validate(&self) -> Result<(), String> {
    if self.frames.is_empty() {
      return Err("workspace must contain a frame".into());
    }
    if self.kind.is_video()
      && self
        .frames
        .iter()
        .any(|f| f.radius_percent.abs() > f64::EPSILON)
    {
      return Err("video frame radius is not supported".into());
    }
    let mut frame_ids = HashSet::new();
    for frame in &self.frames {
      if !frame_ids.insert(frame.id) {
        return Err("duplicate frame id".into());
      }
      if !valid_rect(frame.rect) {
        return Err("invalid frame rectangle".into());
      }
    }
    let mut layer_ids = HashSet::new();
    for layer in &self.layers {
      if !layer_ids.insert(layer.id) {
        return Err("duplicate layer id".into());
      }
      if !frame_ids.contains(&layer.frame_id) {
        return Err("layer references missing frame".into());
      }
      if !valid_normalized(layer.rect) {
        return Err("invalid layer rectangle".into());
      }
    }
    Ok(())
  }

  pub fn frame(&self, id: FrameId) -> Option<&WorkspaceFrame> {
    self.frames.iter().find(|f| f.id == id)
  }

  /// Resize a frame from an immutable snapshot. The returned scene is a new
  /// value; callers can render it immediately and publish its semantic state
  /// separately. Layers retain their absolute workspace geometry while their
  /// normalized coordinates are rebased to the new frame.
  pub fn resized_frame(
    &self,
    frame_id: FrameId,
    edges: u32,
    delta: (f64, f64),
  ) -> Result<FrameResizeResult, String> {
    resize_frame(self, frame_id, edges, delta)
  }
}

fn valid_rect(rect: WorldRect) -> bool {
  rect.x.is_finite()
    && rect.y.is_finite()
    && rect.width.is_finite()
    && rect.height.is_finite()
    && rect.width > 0.0
    && rect.height > 0.0
}
fn valid_normalized(rect: NormalizedRect) -> bool {
  rect.x.is_finite()
    && rect.y.is_finite()
    && rect.width.is_finite()
    && rect.height.is_finite()
    && rect.width > 0.0
    && rect.height > 0.0
}
