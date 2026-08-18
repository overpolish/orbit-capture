//! Platform-neutral model for the native export workspace.
//!
//! This module deliberately contains no renderer or webview code.  Both GPU
//! backends can consume the same frames, layers, hit-test results, and gesture
//! transaction boundaries.  Coordinates in a scene are in workspace points;
//! layer rectangles are normalized to their owning frame.

use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorldRect {
  pub x: f64,
  pub y: f64,
  pub width: f64,
  pub height: f64,
}

impl WorldRect {
  pub fn normalized(self, x: f64, y: f64, width: f64, height: f64) -> Self {
    Self {
      x: self.x + x * self.width,
      y: self.y + y * self.height,
      width: width * self.width,
      height: height * self.height,
    }
  }
  pub fn to_normalized(self, world: WorldRect) -> NormalizedRect {
    NormalizedRect {
      x: (world.x - self.x) / self.width,
      y: (world.y - self.y) / self.height,
      width: world.width / self.width,
      height: world.height / self.height,
    }
  }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NormalizedRect {
  pub x: f64,
  pub y: f64,
  pub width: f64,
  pub height: f64,
}

/// Move a crop window without changing the underlying image transform.
#[cfg(any(target_os = "windows", test))]
pub fn apply_crop_move(
  crop: NormalizedRect,
  image: NormalizedRect,
  delta: (f64, f64),
) -> NormalizedRect {
  let x = (crop.x + delta.0).clamp(image.x, image.x + image.width - crop.width);
  let y = (crop.y + delta.1).clamp(image.y, image.y + image.height - crop.height);
  NormalizedRect { x, y, ..crop }
}

/// Resize a crop window from edge bits (left=1, right=2, top=4, bottom=8).
/// The image bounds constrain the crop but are never modified.
#[cfg(any(target_os = "windows", test))]
pub fn apply_crop_resize(
  crop: NormalizedRect,
  image: NormalizedRect,
  edges: u32,
  delta: (f64, f64),
  centered: bool,
) -> NormalizedRect {
  let image_right = image.x + image.width;
  let image_bottom = image.y + image.height;
  let min_size = 1e-6;
  let mut left = crop.x;
  let mut right = crop.x + crop.width;
  let mut top = crop.y;
  let mut bottom = crop.y + crop.height;
  if edges & FRAME_EDGE_LEFT != 0 {
    let movement = delta.0.clamp(image.x - left, crop.width - min_size);
    left += movement;
    if centered {
      right -= movement;
    }
  } else if edges & FRAME_EDGE_RIGHT != 0 {
    let movement = delta.0.clamp(min_size - crop.width, image_right - right);
    right += movement;
    if centered {
      left -= movement;
    }
  }
  if edges & FRAME_EDGE_TOP != 0 {
    let movement = delta.1.clamp(image.y - top, crop.height - min_size);
    top += movement;
    if centered {
      bottom -= movement;
    }
  } else if edges & FRAME_EDGE_BOTTOM != 0 {
    let movement = delta.1.clamp(min_size - crop.height, image_bottom - bottom);
    bottom += movement;
    if centered {
      top -= movement;
    }
  }
  let width = (right - left).max(min_size).min(image.width.max(min_size));
  let height = (bottom - top).max(min_size).min(image.height.max(min_size));
  left = left.clamp(image.x, image_right - width);
  top = top.clamp(image.y, image_bottom - height);
  NormalizedRect {
    x: left,
    y: top,
    width,
    height,
  }
}

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

/// A target in the coordinate space used by the native preview surface.
/// `z_order` is the back-to-front order; input order breaks equal-z ties.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisplayTarget {
  pub id: u64,
  pub rect: DisplayRect,
  pub radius_enabled: u8,
  pub radius_percent: f64,
  pub z_order: i32,
  pub selected: u8,
  pub visible: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisplayRect {
  pub x: f64,
  pub y: f64,
  pub width: f64,
  pub height: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisplayFitRebase {
  pub fit: DisplayRect,
  pub zoom: f64,
  pub pan_x: f64,
  pub pan_y: f64,
}

/// Re-express an already displayed workspace against a centered fit rect.
/// The returned zoom/pan preserves the current displayed pixels exactly.
pub fn rebase_display_fit(
  viewport: (f64, f64),
  displayed: DisplayRect,
  gutter: f64,
) -> DisplayFitRebase {
  let available_width = (viewport.0 - gutter * 2.0).max(1.0);
  let available_height = (viewport.1 - gutter * 2.0).max(1.0);
  let aspect = displayed.width.max(1.0) / displayed.height.max(1.0);
  let mut fit_width = available_width;
  let mut fit_height = fit_width / aspect.max(0.000_001);
  if fit_height > available_height {
    fit_height = available_height;
    fit_width = fit_height * aspect;
  }
  let fit = DisplayRect {
    x: (viewport.0 - fit_width) / 2.0,
    y: (viewport.1 - fit_height) / 2.0,
    width: fit_width,
    height: fit_height,
  };
  DisplayFitRebase {
    fit,
    zoom: (displayed.width / fit_width.max(1.0)).clamp(0.1, 16.0),
    pan_x: displayed.x + displayed.width / 2.0 - viewport.0 / 2.0,
    pan_y: displayed.y + displayed.height / 2.0 - viewport.1 / 2.0,
  }
}

/// Grow a canvas to contain every layer crop, then express all layer
/// geometry in the new canvas. Absolute crop/image pixels are preserved.
pub fn fit_canvas_to_layers(
  canvas: (u32, u32),
  layers: &[LayerGeometry],
) -> ((u32, u32), Vec<LayerGeometry>) {
  let width = f64::from(canvas.0.max(1));
  let height = f64::from(canvas.1.max(1));
  let mut left = 0.0_f64;
  let mut top = 0.0_f64;
  let mut right = width;
  let mut bottom = height;
  for layer in layers {
    let crop = layer.crop;
    left = left.min((crop.x * width).floor());
    top = top.min((crop.y * height).floor());
    right = right.max(((crop.x + crop.width) * width).ceil());
    bottom = bottom.max(((crop.y + crop.height) * height).ceil());
  }
  let next_width = (right - left).round().max(FRAME_MIN_SIZE) as u32;
  let next_height = (bottom - top).round().max(FRAME_MIN_SIZE) as u32;
  let next_width_f = f64::from(next_width);
  let next_height_f = f64::from(next_height);
  let layers = layers
    .iter()
    .map(|layer| LayerGeometry {
      crop: NormalizedRect {
        x: (layer.crop.x * width - left) / next_width_f,
        y: (layer.crop.y * height - top) / next_height_f,
        width: layer.crop.width * width / next_width_f,
        height: layer.crop.height * height / next_height_f,
      },
      image_center_x: (layer.image_center_x * width - left) / next_width_f,
      image_center_y: (layer.image_center_y * height - top) / next_height_f,
      image_width: layer.image_width * width / next_width_f,
      radius_percent: layer.radius_percent,
    })
    .collect();
  ((next_width, next_height), layers)
}

#[no_mangle]
pub extern "C" fn screenwide_workspace_rebase_display_fit(
  viewport_width: f64,
  viewport_height: f64,
  displayed: DisplayRect,
  gutter: f64,
) -> DisplayFitRebase {
  rebase_display_fit((viewport_width, viewport_height), displayed, gutter)
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayHandle {
  Body = 0,
  North = 1,
  South = 2,
  East = 3,
  West = 4,
  NorthEast = 5,
  NorthWest = 6,
  SouthEast = 7,
  SouthWest = 8,
  Radius = 9,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisplayHit {
  pub found: u8,
  pub target_id: u64,
  pub handle: u8,
}

impl DisplayHit {
  fn new(target_id: u64, handle: DisplayHandle) -> Self {
    Self {
      found: 1,
      target_id,
      handle: handle as u8,
    }
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GestureOperation {
  Move = 0,
  Resize = 1,
  Radius = 2,
}

/// Frame resize edge mask shared by the native input adapters.
///
/// The values intentionally match the existing Metal/D3D gesture protocol:
/// left=1, right=2, top=4, bottom=8 and centered=1<<16.
pub const FRAME_EDGE_LEFT: u32 = 1;
pub const FRAME_EDGE_RIGHT: u32 = 1 << 1;
pub const FRAME_EDGE_TOP: u32 = 1 << 2;
pub const FRAME_EDGE_BOTTOM: u32 = 1 << 3;
pub const FRAME_EDGE_CENTERED: u32 = 1 << 16;
const FRAME_MIN_SIZE: f64 = 64.0;
const FRAME_MAX_AREA: f64 = 120_000_000.0;

#[derive(Clone, Debug)]
pub struct FrameResizeResult {
  pub scene: WorkspaceScene,
  pub old_rect: WorldRect,
  pub new_rect: WorldRect,
  /// Integer output dimensions used by the media/render surface.
  pub output_size: (u32, u32),
}

/// The complete editable geometry of a visual layer, normalized to its frame.
/// Keeping the crop and underlying image transform together prevents pixels
/// and OSCs from being advanced by different gesture equations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerGeometry {
  pub crop: NormalizedRect,
  pub image_center_x: f64,
  pub image_center_y: f64,
  pub image_width: f64,
  pub radius_percent: f64,
}

/// Applies one native pointer update to an immutable layer snapshot. `delta`
/// is the final crop-origin delta already resolved by the platform hit test;
/// `scale` is the final uniform resize factor. This is intentionally the same
/// contract used by the existing Metal and D3D gesture callbacks.
pub fn apply_layer_gesture(
  start: LayerGeometry,
  operation: GestureOperation,
  delta: (f64, f64),
  scale: f64,
) -> LayerGeometry {
  let mut next = start;
  match operation {
    GestureOperation::Move => {
      next.crop.x += delta.0;
      next.crop.y += delta.1;
      next.image_center_x += delta.0;
      next.image_center_y += delta.1;
    }
    GestureOperation::Resize => {
      let scale = scale.clamp(0.0, 8.0);
      let next_x = start.crop.x + delta.0;
      let next_y = start.crop.y + delta.1;
      let transform = |value: f64, start_frame: f64, next_frame: f64| {
        if (scale - 1.0).abs() < 1e-9 {
          value
        } else {
          let anchor = (next_frame - start_frame * scale) / (1.0 - scale);
          anchor + (value - anchor) * scale
        }
      };
      next.crop = NormalizedRect {
        x: next_x,
        y: next_y,
        width: start.crop.width * scale,
        height: start.crop.height * scale,
      };
      next.image_center_x = transform(start.image_center_x, start.crop.x, next_x);
      next.image_center_y = transform(start.image_center_y, start.crop.y, next_y);
      next.image_width = start.image_width * scale;
    }
    GestureOperation::Radius => next.radius_percent = scale.clamp(0.0, 50.0),
  }
  next
}

/// Rebase a normalized layer geometry while preserving its absolute
/// workspace-space crop and image transform.
pub fn rebase_layer_geometry(
  geometry: LayerGeometry,
  old_frame: WorldRect,
  new_frame: WorldRect,
) -> LayerGeometry {
  let crop_world = old_frame.normalized(
    geometry.crop.x,
    geometry.crop.y,
    geometry.crop.width,
    geometry.crop.height,
  );
  let image_center_world =
    old_frame.normalized(geometry.image_center_x, geometry.image_center_y, 0.0, 0.0);
  LayerGeometry {
    crop: new_frame.to_normalized(crop_world),
    image_center_x: (image_center_world.x - new_frame.x) / new_frame.width,
    image_center_y: (image_center_world.y - new_frame.y) / new_frame.height,
    image_width: geometry.image_width * old_frame.width / new_frame.width,
    radius_percent: geometry.radius_percent,
  }
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

impl From<WorldRect> for DisplayRect {
  fn from(rect: WorldRect) -> Self {
    Self {
      x: rect.x,
      y: rect.y,
      width: rect.width,
      height: rect.height,
    }
  }
}

/// Hit-test display-space targets. Only the selected target exposes handle
/// hit regions because it is the only target whose handles are visible.
/// Those handles own their full hit squares before layer bodies, including
/// the portion extending outside the selected layer. Unselected targets are
/// selectable through their bodies without exposing invisible point OSCs.
pub fn hit_test_display(
  targets: &[DisplayTarget],
  point: (f64, f64),
  handle_size: f64,
) -> Option<DisplayHit> {
  let radius = handle_size.max(0.0);
  let mut order: Vec<(usize, &DisplayTarget)> = targets
    .iter()
    .enumerate()
    .filter(|(_, t)| t.visible != 0)
    .collect();
  order.sort_by_key(|(index, target)| (target.z_order, *index));
  // The handles actually drawn for the selected target own their full hit
  // squares, even where those squares overlap a neighbouring layer or one of
  // its currently invisible handles.
  for (_, target) in order
    .iter()
    .rev()
    .filter(|(_, target)| target.selected != 0)
  {
    if let Some(handle) = edge_handle(target.rect, point, radius) {
      return Some(DisplayHit::new(target.id, handle));
    }
  }
  if let Some((_, target)) = order.iter().rev().find(|(_, target)| {
    target.selected != 0
      && target.radius_enabled != 0
      && radius_hit(target.rect, target.radius_percent, point, radius)
  }) {
    return Some(DisplayHit::new(target.id, DisplayHandle::Radius));
  }
  order
    .iter()
    .rev()
    .find(|(_, target)| contains(target.rect, point))
    .map(|(_, target)| DisplayHit::new(target.id, DisplayHandle::Body))
}

fn contains(rect: DisplayRect, point: (f64, f64)) -> bool {
  point.0 >= rect.x
    && point.1 >= rect.y
    && point.0 <= rect.x + rect.width
    && point.1 <= rect.y + rect.height
}

fn edge_handle(rect: DisplayRect, point: (f64, f64), size: f64) -> Option<DisplayHandle> {
  let points = [
    (rect.x, rect.y, DisplayHandle::NorthWest),
    (rect.x + rect.width / 2.0, rect.y, DisplayHandle::North),
    (rect.x + rect.width, rect.y, DisplayHandle::NorthEast),
    (
      rect.x + rect.width,
      rect.y + rect.height / 2.0,
      DisplayHandle::East,
    ),
    (
      rect.x + rect.width,
      rect.y + rect.height,
      DisplayHandle::SouthEast,
    ),
    (
      rect.x + rect.width / 2.0,
      rect.y + rect.height,
      DisplayHandle::South,
    ),
    (rect.x, rect.y + rect.height, DisplayHandle::SouthWest),
    (rect.x, rect.y + rect.height / 2.0, DisplayHandle::West),
  ];
  points
    .into_iter()
    .find(|(x, y, _)| (point.0 - x).abs() <= size && (point.1 - y).abs() <= size)
    .map(|(_, _, handle)| handle)
}

fn radius_hit(rect: DisplayRect, percent: f64, point: (f64, f64), size: f64) -> bool {
  let offset = rect.width.min(rect.height) * percent.clamp(0.0, 50.0) / 100.0 * 0.55 + 10.0;
  (point.0 - rect.x - offset).abs() <= size && (point.1 - rect.y - offset).abs() <= size
}

/// C entry point for native adapters. Null pointers and invalid counts return no hit.
#[no_mangle]
pub unsafe extern "C" fn screenwide_workspace_hit_test(
  targets: *const DisplayTarget,
  count: usize,
  x: f64,
  y: f64,
  handle_size: f64,
) -> DisplayHit {
  if targets.is_null() || count == 0 {
    return DisplayHit::default();
  }
  let targets = unsafe { std::slice::from_raw_parts(targets, count) };
  hit_test_display(targets, (x, y), handle_size).unwrap_or_default()
}

/// Pure frame resize operation used by both native GPU adapters.
pub fn resize_frame(
  source: &WorkspaceScene,
  frame_id: FrameId,
  edges: u32,
  delta: (f64, f64),
) -> Result<FrameResizeResult, String> {
  let old_frame = source.frame(frame_id).ok_or("frame missing")?.clone();
  let horizontal = edges & (FRAME_EDGE_LEFT | FRAME_EDGE_RIGHT) != 0;
  let vertical = edges & (FRAME_EDGE_TOP | FRAME_EDGE_BOTTOM) != 0;
  if !horizontal && !vertical {
    return Err("frame resize has no edges".into());
  }
  if !delta.0.is_finite() || !delta.1.is_finite() {
    return Err("frame resize delta is not finite".into());
  }

  let centered = edges & FRAME_EDGE_CENTERED != 0;
  let left = edges & FRAME_EDGE_LEFT != 0;
  let right = edges & FRAME_EDGE_RIGHT != 0;
  let top = edges & FRAME_EDGE_TOP != 0;
  let bottom = edges & FRAME_EDGE_BOTTOM != 0;
  let (mut x, mut width) = resized_axis(
    old_frame.rect.x,
    old_frame.rect.width,
    delta.0,
    left,
    right,
    centered,
  );
  let (mut y, mut height) = resized_axis(
    old_frame.rect.y,
    old_frame.rect.height,
    delta.1,
    top,
    bottom,
    centered,
  );

  // Frame dimensions correspond to output pixels. Round before applying the
  // safety bounds so Metal and D3D receive identical integer sizes.
  width = width.round().max(FRAME_MIN_SIZE);
  height = height.round().max(FRAME_MIN_SIZE);
  let area = width * height;
  if area > FRAME_MAX_AREA {
    let factor = (FRAME_MAX_AREA / area).sqrt();
    width = (width * factor).floor().max(FRAME_MIN_SIZE);
    height = (height * factor).floor().max(FRAME_MIN_SIZE);
  }
  // Re-anchor the opposite edge (or the center for centered resizing) after
  // integer rounding and max-area limiting.
  x = anchored_origin(
    old_frame.rect.x,
    old_frame.rect.width,
    x,
    width,
    left,
    right,
    centered,
  );
  y = anchored_origin(
    old_frame.rect.y,
    old_frame.rect.height,
    y,
    height,
    top,
    bottom,
    centered,
  );
  let new_rect = WorldRect {
    x,
    y,
    width,
    height,
  };

  let mut next = source.clone();
  let old_rect = old_frame.rect;
  if let Some(frame) = next.frames.iter_mut().find(|frame| frame.id == frame_id) {
    frame.rect = new_rect;
  }
  for layer in &mut next.layers {
    if layer.frame_id != frame_id {
      continue;
    }
    let world = old_rect.normalized(
      layer.rect.x,
      layer.rect.y,
      layer.rect.width,
      layer.rect.height,
    );
    layer.rect = new_rect.to_normalized(world);
  }
  next.revision = source.revision.saturating_add(1);
  next.validate()?;
  Ok(FrameResizeResult {
    scene: next,
    old_rect,
    new_rect,
    output_size: (new_rect.width as u32, new_rect.height as u32),
  })
}

fn resized_axis(
  origin: f64,
  size: f64,
  delta: f64,
  min_edge: bool,
  max_edge: bool,
  centered: bool,
) -> (f64, f64) {
  if centered && (min_edge || max_edge) {
    let amount = if min_edge { -delta } else { delta };
    (origin - amount, size + amount * 2.0)
  } else if min_edge && !max_edge {
    (origin + delta, size - delta)
  } else if max_edge && !min_edge {
    (origin, size + delta)
  } else {
    (origin, size)
  }
}

fn anchored_origin(
  old_origin: f64,
  old_size: f64,
  proposed_origin: f64,
  new_size: f64,
  min_edge: bool,
  max_edge: bool,
  centered: bool,
) -> f64 {
  if centered && (min_edge || max_edge) {
    old_origin + (old_size - new_size) / 2.0
  } else if min_edge && !max_edge {
    old_origin + old_size - new_size
  } else {
    proposed_origin
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

#[cfg(test)]
mod tests {
  use super::*;

  fn target(id: u64, rect: DisplayRect, z_order: i32, selected: bool) -> DisplayTarget {
    DisplayTarget {
      id,
      rect,
      z_order,
      selected: u8::from(selected),
      visible: 1,
      radius_enabled: 1,
      radius_percent: 20.0,
    }
  }

  #[test]
  fn inactive_target_edge_only_selects_its_body() {
    let targets = [target(
      1,
      DisplayRect {
        x: 50.0,
        y: 0.0,
        width: 50.0,
        height: 100.0,
      },
      0,
      false,
    )];
    let hit = hit_test_display(&targets, (50.0, 50.0), 8.0).unwrap();
    assert_eq!((hit.target_id, hit.handle), (1, DisplayHandle::Body as u8));
  }

  #[test]
  fn overlapping_body_picks_top_layer() {
    let targets = [
      target(
        1,
        DisplayRect {
          x: 0.0,
          y: 0.0,
          width: 100.0,
          height: 100.0,
        },
        1,
        false,
      ),
      target(
        2,
        DisplayRect {
          x: 20.0,
          y: 20.0,
          width: 80.0,
          height: 80.0,
        },
        2,
        false,
      ),
    ];
    assert_eq!(
      hit_test_display(&targets, (50.0, 50.0), 4.0)
        .unwrap()
        .target_id,
      2
    );
  }

  #[test]
  fn selected_resize_handle_wins_over_neighbouring_target() {
    let targets = [
      target(
        1,
        DisplayRect {
          x: 0.0,
          y: 0.0,
          width: 100.0,
          height: 100.0,
        },
        0,
        true,
      ),
      target(
        2,
        DisplayRect {
          x: 92.0,
          y: 20.0,
          width: 80.0,
          height: 80.0,
        },
        1,
        false,
      ),
    ];
    // The selected target's east handle overlaps both target 2's body and its
    // invisible west-handle region. The visible selected handle must win.
    let hit = hit_test_display(&targets, (99.0, 57.0), 8.0).unwrap();
    assert_eq!((hit.target_id, hit.handle), (1, DisplayHandle::East as u8));
  }

  #[test]
  fn selected_radius_point_precedes_other_handles() {
    let targets = [target(
      7,
      DisplayRect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
      },
      0,
      true,
    )];
    let hit = hit_test_display(&targets, (21.0, 21.0), 8.0).unwrap();
    assert_eq!(hit.handle, DisplayHandle::Radius as u8);
  }

  #[test]
  fn disabled_video_radius_falls_back_to_body_or_resize() {
    let mut video = target(
      3,
      DisplayRect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
      },
      0,
      true,
    );
    video.radius_enabled = 0;
    let hit = hit_test_display(&[video], (21.0, 21.0), 8.0).unwrap();
    assert_eq!(hit.handle, DisplayHandle::Body as u8);
  }
  fn r(x: f64, y: f64, width: f64, height: f64) -> WorldRect {
    WorldRect {
      x,
      y,
      width,
      height,
    }
  }
  fn n(x: f64, y: f64, width: f64, height: f64) -> NormalizedRect {
    NormalizedRect {
      x,
      y,
      width,
      height,
    }
  }
  fn geometry(crop: NormalizedRect) -> LayerGeometry {
    LayerGeometry {
      crop,
      image_center_x: crop.x + crop.width / 2.0,
      image_center_y: crop.y + crop.height / 2.0,
      image_width: crop.width,
      radius_percent: 0.0,
    }
  }
  fn layer(id: u32, frame_id: u32, z_index: i32) -> WorkspaceLayer {
    WorkspaceLayer {
      id: LayerId(id),
      frame_id: FrameId(frame_id),
      rect: n(0.1, 0.1, 0.5, 0.5),
      radius_percent: 0.0,
      z_index,
    }
  }
  #[test]
  fn screenshot_is_one_frame_with_multiple_layers() {
    let s = WorkspaceScene::screenshot(
      r(0.0, 0.0, 100.0, 100.0),
      r(0.0, 0.0, 100.0, 100.0),
      vec![layer(1, 0, 0), layer(2, 0, 1)],
    )
    .unwrap();
    assert_eq!(s.frames.len(), 1);
    assert_eq!(s.layers.len(), 2);
  }
  #[test]
  fn baked_video_is_one_frame() {
    let s = WorkspaceScene::baked_video(
      r(0.0, 0.0, 100.0, 100.0),
      r(0.0, 0.0, 100.0, 100.0),
      vec![layer(1, 0, 0)],
    )
    .unwrap();
    assert_eq!(s.kind, WorkspaceKind::BakedVideo);
  }
  #[test]
  fn split_video_has_two_independent_frames() {
    let frames = vec![
      WorkspaceFrame {
        id: FrameId(0),
        rect: r(0.0, 0.0, 50.0, 100.0),
        radius_percent: 0.0,
      },
      WorkspaceFrame {
        id: FrameId(1),
        rect: r(50.0, 0.0, 50.0, 100.0),
        radius_percent: 0.0,
      },
    ];
    let s =
      WorkspaceScene::split_video(r(0.0, 0.0, 100.0, 100.0), frames, vec![layer(1, 1, 0)]).unwrap();
    assert_eq!(s.frames.len(), 2);
    assert_eq!(
      s.frames[1]
        .rect
        .normalized(s.layers[0].rect.x, 0.0, 0.0, 0.0)
        .x,
      55.0
    );
  }
  #[test]
  fn layer_resize_uses_one_transform_for_crop_and_image() {
    let start = geometry(n(0.2, 0.3, 0.4, 0.5));
    let result = apply_layer_gesture(start, GestureOperation::Resize, (0.1, -0.1), 0.5);
    assert!((result.crop.x - 0.3).abs() < 1e-9);
    assert!((result.crop.y - 0.2).abs() < 1e-9);
    assert_eq!((result.crop.width, result.crop.height), (0.2, 0.25));
    assert_eq!(result.image_width, 0.2);
    assert!((result.image_center_x - 0.4).abs() < 1e-9);
    assert!((result.image_center_y - 0.325).abs() < 1e-9);
  }
  #[test]
  fn layer_gesture_allows_off_canvas_move() {
    let result = apply_layer_gesture(
      geometry(n(0.1, 0.1, 0.2, 0.2)),
      GestureOperation::Move,
      (-1.0, 2.0),
      1.0,
    );
    assert_eq!((result.crop.x, result.crop.y), (-0.9, 2.1));
    assert_eq!((result.image_center_x, result.image_center_y), (-0.8, 2.2));
  }
  #[test]
  fn frame_resize_top_edge_is_undo_symmetric() {
    let s = WorkspaceScene::screenshot(
      r(0.0, 0.0, 400.0, 400.0),
      r(20.0, 30.0, 200.0, 160.0),
      vec![],
    )
    .unwrap();
    let grown = s
      .resized_frame(FrameId(0), FRAME_EDGE_TOP, (-0.0, -20.0))
      .unwrap();
    let restored = grown
      .scene
      .resized_frame(FrameId(0), FRAME_EDGE_TOP, (0.0, 20.0))
      .unwrap();
    assert_eq!(restored.new_rect, s.frame(FrameId(0)).unwrap().rect);
    assert_eq!(restored.output_size, (200, 160));
  }
  #[test]
  fn split_resize_only_changes_selected_frame() {
    let frames = vec![
      WorkspaceFrame {
        id: FrameId(0),
        rect: r(0.0, 0.0, 200.0, 200.0),
        radius_percent: 0.0,
      },
      WorkspaceFrame {
        id: FrameId(1),
        rect: r(220.0, 0.0, 200.0, 200.0),
        radius_percent: 0.0,
      },
    ];
    let s = WorkspaceScene::split_video(r(0.0, 0.0, 500.0, 300.0), frames, vec![]).unwrap();
    let result = s
      .resized_frame(FrameId(1), FRAME_EDGE_RIGHT, (40.0, 0.0))
      .unwrap();
    assert_eq!(
      result.scene.frame(FrameId(0)).unwrap().rect,
      s.frame(FrameId(0)).unwrap().rect
    );
    assert_eq!(result.new_rect.width, 240.0);
  }
  #[test]
  fn baked_resize_rebases_layer_without_moving_pixels() {
    let layer = WorkspaceLayer {
      id: LayerId(1),
      frame_id: FrameId(0),
      rect: n(0.25, 0.25, 0.5, 0.5),
      radius_percent: 0.0,
      z_index: 0,
    };
    let s = WorkspaceScene::baked_video(
      r(0.0, 0.0, 400.0, 400.0),
      r(100.0, 100.0, 200.0, 200.0),
      vec![layer],
    )
    .unwrap();
    let before = s.frames[0].rect.normalized(
      s.layers[0].rect.x,
      s.layers[0].rect.y,
      s.layers[0].rect.width,
      s.layers[0].rect.height,
    );
    let result = s
      .resized_frame(FrameId(0), FRAME_EDGE_RIGHT, (100.0, 0.0))
      .unwrap();
    let after = result.scene.frames[0].rect.normalized(
      result.scene.layers[0].rect.x,
      result.scene.layers[0].rect.y,
      result.scene.layers[0].rect.width,
      result.scene.layers[0].rect.height,
    );
    assert_eq!(after, before);
    assert_eq!(result.scene.layers[0].rect.x, 1.0 / 6.0);
  }
  #[test]
  fn centered_resize_keeps_frame_center() {
    let s = WorkspaceScene::screenshot(
      r(0.0, 0.0, 400.0, 400.0),
      r(100.0, 100.0, 200.0, 200.0),
      vec![],
    )
    .unwrap();
    let result = s
      .resized_frame(
        FrameId(0),
        FRAME_EDGE_RIGHT | FRAME_EDGE_CENTERED,
        (20.0, 0.0),
      )
      .unwrap();
    assert_eq!(result.new_rect, r(80.0, 100.0, 240.0, 200.0));
  }
  #[test]
  fn frame_resize_respects_max_area() {
    let s = WorkspaceScene::screenshot(
      r(0.0, 0.0, 20_000.0, 20_000.0),
      r(0.0, 0.0, 12_000.0, 12_000.0),
      vec![],
    )
    .unwrap();
    let result = s
      .resized_frame(
        FrameId(0),
        FRAME_EDGE_RIGHT | FRAME_EDGE_BOTTOM,
        (10_000.0, 10_000.0),
      )
      .unwrap();
    assert!(result.new_rect.width * result.new_rect.height <= FRAME_MAX_AREA);
    assert!(result.output_size.0 >= FRAME_MIN_SIZE as u32);
  }
  #[test]
  fn layer_geometry_rebase_preserves_absolute_image_transform() {
    let old = r(100.0, 50.0, 200.0, 100.0);
    let new = r(50.0, 25.0, 400.0, 200.0);
    let start = LayerGeometry {
      crop: n(0.25, 0.25, 0.5, 0.5),
      image_center_x: 0.5,
      image_center_y: 0.5,
      image_width: 0.75,
      radius_percent: 12.0,
    };
    let rebased = rebase_layer_geometry(start, old, new);
    assert_eq!(rebased.crop, n(0.25, 0.25, 0.25, 0.25));
    assert_eq!(
      (rebased.image_center_x, rebased.image_center_y),
      (0.375, 0.375)
    );
    assert_eq!(rebased.image_width, 0.375);
    assert_eq!(rebased.radius_percent, 12.0);
  }

  #[test]
  fn display_fit_rebase_preserves_displayed_bounds() {
    let displayed = DisplayRect {
      x: 140.0,
      y: 100.0,
      width: 720.0,
      height: 360.0,
    };
    let rebased = rebase_display_fit((1_000.0, 700.0), displayed, 8.0);
    assert_eq!(
      rebased.fit,
      DisplayRect {
        x: 8.0,
        y: 104.0,
        width: 984.0,
        height: 492.0,
      }
    );
    assert!((rebased.zoom - 720.0 / 984.0).abs() < 0.000_001);
    assert_eq!(rebased.pan_x, 0.0);
    assert_eq!(rebased.pan_y, -70.0);
  }

  #[test]
  fn canvas_fit_preserves_absolute_layer_geometry() {
    let layer = LayerGeometry {
      crop: n(0.25, -0.5, 0.5, 1.0),
      image_center_x: 0.5,
      image_center_y: 0.0,
      image_width: 0.75,
      radius_percent: 12.0,
    };
    let ((width, height), layers) = fit_canvas_to_layers((400, 200), &[layer]);
    assert_eq!((width, height), (400, 300));
    let fitted = layers[0];
    assert_eq!(fitted.crop, n(0.25, 0.0, 0.5, 2.0 / 3.0));
    assert_eq!(fitted.image_center_x, 0.5);
    assert_eq!(fitted.image_center_y, 1.0 / 3.0);
    assert_eq!(fitted.image_width, 0.75);
    assert_eq!(fitted.radius_percent, 12.0);
  }

  #[test]
  fn crop_move_is_clamped_without_changing_size() {
    let crop = n(0.2, 0.25, 0.4, 0.5);
    let image = n(0.1, 0.1, 0.8, 0.8);
    let moved = apply_crop_move(crop, image, (1.0, -1.0));
    assert_eq!(moved, n(0.5, 0.1, 0.4, 0.5));
  }

  #[test]
  fn crop_resize_is_clamped_to_image_and_preserves_image() {
    let crop = n(0.2, 0.2, 0.4, 0.4);
    let image = n(0.1, 0.1, 0.8, 0.8);
    let resized = apply_crop_resize(
      crop,
      image,
      FRAME_EDGE_RIGHT | FRAME_EDGE_BOTTOM,
      (1.0, 1.0),
      false,
    );
    assert_eq!(resized, n(0.2, 0.2, 0.7, 0.7));
    let centered = apply_crop_resize(
      crop,
      image,
      FRAME_EDGE_RIGHT | FRAME_EDGE_BOTTOM,
      (0.1, 0.1),
      true,
    );
    assert!((centered.x - 0.1).abs() < 1e-9);
    assert!((centered.y - 0.1).abs() < 1e-9);
    assert!((centered.width - 0.6).abs() < 1e-9);
    assert!((centered.height - 0.6).abs() < 1e-9);
  }
}
