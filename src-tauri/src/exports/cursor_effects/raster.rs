// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::recording::cursor::CursorStyle;
use image::RgbaImage;
use rayon::prelude::*;

#[cfg(target_os = "macos")]
#[path = "raster/platform_macos.rs"]
mod platform;
#[cfg(not(target_os = "macos"))]
#[path = "raster/platform_unsupported.rs"]
mod platform;

mod fallback;

pub(super) fn uses_same_artwork(left: CursorStyle, right: CursorStyle) -> bool {
  // A custom cursor is recorded without its pixels, so it draws the system
  // arrow fitted inside its recorded box and anchored by the arrow's own
  // hotspot, not stretched over the box from the recorded one. That is a
  // different mapping from the `Arrow` style even though the bitmap matches,
  // so collapsing the two would hold a custom cursor's box and hotspot over
  // the arrow that follows it.
  if (left == CursorStyle::Custom) != (right == CursorStyle::Custom) {
    return false;
  }
  platform::canonical_style(left) == platform::canonical_style(right)
}

pub(super) fn initialize_system_artwork() {
  platform::initialize();
}

/// Artwork order shared by the GPU compositors' style-indexed textures and
/// `GpuCursor::style`. Every system style that resolves to its own artwork
/// appears once; `Custom` takes the extra slot after them.
pub(super) const GPU_ARTWORK_STYLES: [CursorStyle; 16] = [
  CursorStyle::Arrow,
  CursorStyle::ClosedHand,
  CursorStyle::ContextMenu,
  CursorStyle::Crosshair,
  CursorStyle::DisappearingItem,
  CursorStyle::DragCopy,
  CursorStyle::DragLink,
  CursorStyle::IBeam,
  CursorStyle::NotAllowed,
  CursorStyle::OpenHand,
  CursorStyle::PointingHand,
  CursorStyle::ResizeHorizontal,
  CursorStyle::ResizeVertical,
  CursorStyle::VerticalIBeam,
  CursorStyle::ZoomIn,
  CursorStyle::ZoomOut,
];

/// The slot after the system styles, holding the artwork a custom cursor
/// draws. The sidecar records a custom cursor's box and hotspot but none of
/// its pixels, and that box has no reason to share the system arrow's aspect,
/// so stretching the arrow over it squashes the drawn cursor. This slot fits
/// the system arrow inside the recorded box at the arrow's own aspect instead,
/// anchored by the arrow's own hotspot, and falls back to the baked vector
/// arrow only where the system artwork could not be loaded at all.
pub(super) const GPU_CUSTOM_ARTWORK_INDEX: u32 = GPU_ARTWORK_STYLES.len() as u32;

pub(super) fn artwork_index(style: CursorStyle) -> u32 {
  if style == CursorStyle::Custom {
    return GPU_CUSTOM_ARTWORK_INDEX;
  }
  let style = platform::canonical_style(style);
  GPU_ARTWORK_STYLES
    .iter()
    .position(|candidate| *candidate == style)
    .unwrap_or(0) as u32
}

/// One style's artwork bitmap plus the mapping the shader needs to place it,
/// mirroring exactly what [`CursorRaster::sample`] does on the CPU. A system
/// style's artwork stretches over the recorded cursor box; artwork placed by
/// its own design frame (the vector fallback, and the arrow a custom cursor
/// draws) keeps that frame's aspect inside the box and anchors `origin` at the
/// recorded position, so the frame and origin travel with it.
#[cfg(target_os = "macos")]
pub(in crate::exports) struct GpuArtwork {
  pub design_height: f32,
  pub design_width: f32,
  pub height: u32,
  pub origin_x: f32,
  pub origin_y: f32,
  pub pixels: Vec<u8>,
  /// The fallback arrow deliberately draws outside the recorded cursor box so
  /// its rounded tip stroke survives at the hotspot (`sample`, raster.rs:85).
  pub clip_local_box: bool,
  pub use_design: bool,
  pub vertical: bool,
  pub width: u32,
}

/// Texels per design unit when the vector fallback is baked for the GPU. The
/// shader supersamples the same 4x4 box `sample_for_draw` uses, so the bake
/// stays point-sampled rather than pre-filtered.
#[cfg(target_os = "macos")]
const FALLBACK_BAKE_SCALE: u32 = 8;

#[cfg(target_os = "macos")]
pub(super) fn gpu_artworks() -> Vec<GpuArtwork> {
  GPU_ARTWORK_STYLES
    .iter()
    .copied()
    .map(gpu_artwork)
    // `GPU_CUSTOM_ARTWORK_INDEX`: a custom cursor draws the system arrow
    // fitted inside its recorded box, never that arrow stretched over the box.
    .chain(std::iter::once(custom_gpu_artwork(
      platform::style_artwork(CursorStyle::Arrow),
    )))
    .collect()
}

/// The artwork a custom cursor draws. Taking the arrow entry as an argument
/// keeps the routing testable in a process that never loaded system artwork.
///
/// Geometry, matching [`CursorRaster::sample`] and the Windows compositor's
/// `native_cursor_hotspots` rule (preview_platform/surface_windows
/// /compositor.rs:118):
/// - the design frame is the arrow bitmap itself, so the shader's `use_design`
///   path scales it by `min(box_width / frame_width, box_height / frame_height)`
///   and never stretches it to the recorded box's aspect;
/// - `origin` is the arrow's own hotspot, which the shader adds after that
///   scale, so the arrow is anchored by its hotspot at the recorded position.
///   The recorded hotspot addresses artwork that is not being drawn and stays
///   out of it (`output_hotspot`, cursor_effects.rs:94);
/// - the fitted arrow reaches outside the recorded box on the hotspot's side,
///   so only the design frame clips it.
#[cfg(target_os = "macos")]
fn custom_gpu_artwork(arrow: Option<&platform::StyleArtwork>) -> GpuArtwork {
  // Last resort: with no system artwork at all (headless, tests) the baked
  // vector arrow still draws by exactly the same rules.
  let Some(arrow) = arrow else {
    return fallback_gpu_artwork(CursorStyle::Custom);
  };
  GpuArtwork {
    design_height: arrow.image.height() as f32,
    design_width: arrow.image.width() as f32,
    height: arrow.image.height(),
    origin_x: arrow.hotspot_x as f32,
    origin_y: arrow.hotspot_y as f32,
    pixels: arrow.image.as_raw().clone(),
    clip_local_box: false,
    use_design: true,
    vertical: false,
    width: arrow.image.width(),
  }
}

#[cfg(target_os = "macos")]
fn gpu_artwork(style: CursorStyle) -> GpuArtwork {
  if let Some(image) = platform::artwork(style) {
    return GpuArtwork {
      design_height: 0.0,
      design_width: 0.0,
      height: image.height(),
      origin_x: 0.0,
      origin_y: 0.0,
      pixels: image.as_raw().clone(),
      clip_local_box: true,
      use_design: false,
      vertical: false,
      width: image.width(),
    };
  }
  fallback_gpu_artwork(style)
}

#[cfg(target_os = "macos")]
fn fallback_gpu_artwork(style: CursorStyle) -> GpuArtwork {
  let artwork = fallback::artwork(style);
  // The design frames and origin come from `CursorRaster::sample`
  // (raster.rs:93-108); the bake reproduces its `fallback::sample` lookups.
  let (design_width, design_height) = if artwork == fallback::Artwork::Hand {
    (32.0_f64, 32.0_f64)
  } else {
    (28.0_f64, 40.0_f64)
  };
  let (origin_x, origin_y) = fallback::origin(artwork);
  let width = design_width as u32 * FALLBACK_BAKE_SCALE;
  let height = design_height as u32 * FALLBACK_BAKE_SCALE;
  let mut pixels = vec![0_u8; width as usize * height as usize * 4];
  for y in 0..height {
    for x in 0..width {
      // `sample_image` addresses texel `i` at coordinate `i`, so the bake
      // must place its samples on that same grid rather than at texel centres.
      let sample = fallback::sample(
        artwork,
        f64::from(x) / f64::from(FALLBACK_BAKE_SCALE),
        f64::from(y) / f64::from(FALLBACK_BAKE_SCALE),
      );
      let offset = (y as usize * width as usize + x as usize) * 4;
      for channel in 0..4 {
        pixels[offset + channel] = sample[channel].round().clamp(0.0, 255.0) as u8;
      }
    }
  }
  GpuArtwork {
    design_height: design_height as f32,
    design_width: design_width as f32,
    height,
    origin_x: origin_x as f32,
    origin_y: origin_y as f32,
    pixels,
    clip_local_box: artwork != fallback::Artwork::Arrow,
    use_design: true,
    vertical: fallback::is_vertical(style),
    width,
  }
}

/// Places system artwork by its own design frame instead of stretching it over
/// the recorded cursor box: the frame is fitted into the box at a single
/// aspect-preserving scale and `origin` (the artwork's own hotspot, in frame
/// units) lands at the recorded position. Only a custom cursor uses this — its
/// pixels are not recorded, so neither its box's aspect nor its hotspot
/// describes the arrow that stands in for it.
#[derive(Clone, Copy)]
struct SystemDesign {
  height: f64,
  origin_x: f64,
  origin_y: f64,
  width: f64,
}

#[derive(Clone, Copy)]
pub(super) struct CursorRaster {
  artwork: fallback::Artwork,
  cos: f64,
  height: f64,
  hotspot_x: f64,
  hotspot_y: f64,
  scale: f64,
  sin: f64,
  system_artwork: Option<&'static RgbaImage>,
  system_design: Option<SystemDesign>,
  width: f64,
}

pub(super) struct FrameMut<'a> {
  pub height: usize,
  pub pixels: &'a mut [u8],
  pub stride: usize,
  pub width: usize,
}

impl CursorRaster {
  #[allow(clippy::too_many_arguments)]
  pub(super) fn new(
    style: CursorStyle,
    rotation_degrees: f64,
    width: f64,
    height: f64,
    hotspot_x: f64,
    hotspot_y: f64,
    scale: f64,
  ) -> Self {
    // A custom cursor's pixels are not recorded, and its box has no reason to
    // share the system arrow's aspect, so it draws the system arrow fitted
    // inside that box and anchored by the arrow's own hotspot rather than
    // stretched over it. The preview player rasterises through here and must
    // match the GPU export (`GPU_CUSTOM_ARTWORK_INDEX`, `custom_gpu_artwork`).
    let entry = platform::style_artwork(style);
    let system_artwork = entry.map(|entry| &entry.image);
    let system_design = entry
      .filter(|_| style == CursorStyle::Custom)
      .map(|entry| SystemDesign {
        height: f64::from(entry.image.height()),
        origin_x: entry.hotspot_x,
        origin_y: entry.hotspot_y,
        width: f64::from(entry.image.width()),
      });
    let vertical = system_artwork.is_none() && fallback::is_vertical(style);
    #[cfg(target_os = "windows")]
    let rotation_degrees = -rotation_degrees;
    let rotation = rotation_degrees.to_radians()
      + if vertical {
        std::f64::consts::FRAC_PI_2
      } else {
        0.0
      };
    let (sin, cos) = rotation.sin_cos();
    Self {
      artwork: fallback::artwork(style),
      cos,
      height,
      hotspot_x,
      hotspot_y,
      scale,
      sin,
      system_artwork,
      system_design,
      width,
    }
  }

  fn sample(self, destination_x: f64, destination_y: f64, x: f64, y: f64) -> [f64; 4] {
    let dx = destination_x - x;
    let dy = destination_y - y;
    let local_x = (self.cos * dx + self.sin * dy) / self.scale + self.hotspot_x;
    let local_y = (-self.sin * dx + self.cos * dy) / self.scale + self.hotspot_y;
    let fallback_arrow = self.system_artwork.is_none() && self.artwork == fallback::Artwork::Arrow;
    // Artwork fitted by its own design frame is anchored by that frame's
    // hotspot, so it reaches outside the recorded box and is clipped by the
    // frame below instead; the fallback arrow does the same for its tip stroke.
    if !fallback_arrow
      && self.system_design.is_none()
      && (!(0.0..self.width).contains(&local_x) || !(0.0..self.height).contains(&local_y))
    {
      return [0.0; 4];
    }
    if let (Some(artwork), Some(design)) = (self.system_artwork, self.system_design) {
      // Aspect-preserving fit into the recorded box, anchored by the artwork's
      // own hotspot. Ported to the GPU by `custom_gpu_artwork` and the shader's
      // `use_design` path (gpu_compositor_macos.m `cursor_artwork_sample`).
      let artwork_scale = (self.width / design.width)
        .min(self.height / design.height)
        .max(0.01);
      let design_x = local_x / artwork_scale + design.origin_x;
      let design_y = local_y / artwork_scale + design.origin_y;
      if !(0.0..design.width).contains(&design_x) || !(0.0..design.height).contains(&design_y) {
        return [0.0; 4];
      }
      return sample_image(
        artwork,
        design_x / design.width * f64::from(artwork.width()),
        design_y / design.height * f64::from(artwork.height()),
      );
    }
    self.system_artwork.map_or_else(
      || {
        let design_size = if self.artwork == fallback::Artwork::Hand {
          (32.0, 32.0)
        } else {
          (28.0, 40.0)
        };
        let artwork_scale = (self.width / design_size.0)
          .min(self.height / design_size.1)
          .max(0.01);
        let (origin_x, origin_y) = fallback::origin(self.artwork);
        let design_x = local_x / artwork_scale + origin_x;
        let design_y = local_y / artwork_scale + origin_y;
        if fallback_arrow && (!(0.0..28.0).contains(&design_x) || !(0.0..40.0).contains(&design_y))
        {
          return [0.0; 4];
        }
        fallback::sample(self.artwork, design_x, design_y)
      },
      |artwork| {
        sample_image(
          artwork,
          local_x / self.width * f64::from(artwork.width()),
          local_y / self.height * f64::from(artwork.height()),
        )
      },
    )
  }

  fn sample_for_draw(self, destination_x: f64, destination_y: f64, x: f64, y: f64) -> [f64; 4] {
    // The shader supersamples every artwork it places by a design frame
    // (`cursor_draw_sample`, gpu_compositor_macos.m), because that path clips
    // against the frame's hard edge; stretched system artwork carries its own
    // antialiased edge and is sampled once. The preview follows the same split.
    if self.system_artwork.is_some() && self.system_design.is_none() {
      return self.sample(destination_x, destination_y, x, y);
    }

    const OFFSETS: [f64; 4] = [-0.375, -0.125, 0.125, 0.375];
    let mut alpha = 0.0;
    let mut color = [0.0; 3];
    for offset_y in OFFSETS {
      for offset_x in OFFSETS {
        let source = self.sample(destination_x + offset_x, destination_y + offset_y, x, y);
        let sample_alpha = source[3] / 255.0;
        alpha += sample_alpha;
        for channel in 0..3 {
          color[channel] += source[channel] * sample_alpha;
        }
      }
    }
    if alpha <= 0.0 {
      return [0.0; 4];
    }
    for channel in &mut color {
      *channel /= alpha;
    }
    [color[0], color[1], color[2], alpha / 16.0 * 255.0]
  }

  fn radius(self) -> f64 {
    self.width.hypot(self.height) * self.scale
  }
}

fn sample_image(image: &RgbaImage, x: f64, y: f64) -> [f64; 4] {
  let x = x.clamp(0.0, f64::from(image.width().saturating_sub(1)));
  let y = y.clamp(0.0, f64::from(image.height().saturating_sub(1)));
  let x0 = x.floor() as u32;
  let y0 = y.floor() as u32;
  let x1 = (x0 + 1).min(image.width() - 1);
  let y1 = (y0 + 1).min(image.height() - 1);
  let fraction_x = x - f64::from(x0);
  let fraction_y = y - f64::from(y0);
  let samples = [
    (
      image.get_pixel(x0, y0).0,
      (1.0 - fraction_x) * (1.0 - fraction_y),
    ),
    (image.get_pixel(x1, y0).0, fraction_x * (1.0 - fraction_y)),
    (image.get_pixel(x0, y1).0, (1.0 - fraction_x) * fraction_y),
    (image.get_pixel(x1, y1).0, fraction_x * fraction_y),
  ];
  let mut alpha = 0.0;
  let mut color = [0.0; 3];
  for (sample, weight) in samples {
    let sample_alpha = f64::from(sample[3]) / 255.0;
    alpha += sample_alpha * weight;
    for channel in 0..3 {
      color[channel] += f64::from(sample[channel]) * sample_alpha * weight;
    }
  }
  if alpha <= 0.0 {
    return [0.0; 4];
  }
  [
    color[0] / alpha,
    color[1] / alpha,
    color[2] / alpha,
    alpha * 255.0,
  ]
}

fn blend(frame: &mut FrameMut<'_>, x: usize, y: usize, source: [f64; 4]) {
  let offset = y * frame.stride;
  let Some(row) = frame.pixels.get_mut(offset..offset + frame.stride) else {
    return;
  };
  blend_row(row, x, source);
}

fn blend_row(row: &mut [u8], x: usize, source: [f64; 4]) {
  let source_alpha = source[3].clamp(0.0, 1.0);
  if source_alpha <= 0.0 {
    return;
  }
  let offset = x * 4;
  if offset + 3 >= row.len() {
    return;
  }
  let destination_alpha = f64::from(row[offset + 3]) / 255.0;
  let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
  for (destination, source) in row[offset..offset + 3]
    .iter_mut()
    .zip([source[2], source[1], source[0]])
  {
    let premultiplied =
      source * source_alpha + f64::from(*destination) * destination_alpha * (1.0 - source_alpha);
    *destination = (premultiplied / output_alpha).round() as u8;
  }
  row[offset + 3] = (output_alpha * 255.0).round() as u8;
}

pub(super) fn draw(frame: &mut FrameMut<'_>, cursor: CursorRaster, x: f64, y: f64) {
  let radius = cursor.radius();
  let bounds = bounds(frame, x, y, radius, 0.0);
  for destination_y in bounds.1..bounds.3 {
    for destination_x in bounds.0..bounds.2 {
      // The native cursor artwork already carries an antialiased alpha edge,
      // and `sample_image` interpolates it while scaling and rotating. A 4x4
      // supersample here did the same texture lookup sixteen times per output
      // pixel, which made a large cursor layer slower than the recording it
      // was being composited onto without improving its edge.
      let mut source =
        cursor.sample_for_draw(destination_x as f64 + 0.5, destination_y as f64 + 0.5, x, y);
      source[3] /= 255.0;
      blend(frame, destination_x, destination_y, source);
    }
  }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_blurred(
  frame: &mut FrameMut<'_>,
  cursor: CursorRaster,
  x: f64,
  y: f64,
  direction_x: f64,
  direction_y: f64,
  distance: f64,
  sample_count: usize,
) {
  let bounds = bounds(frame, x, y, cursor.radius(), distance);
  let weights = (0..sample_count)
    .map(|index| {
      let progress = index as f64 / (sample_count - 1) as f64;
      let centered = (progress - 0.5) / 0.34;
      (-0.5 * centered * centered).exp()
    })
    .collect::<Vec<_>>();
  let total_weight = weights.iter().sum::<f64>();
  frame
    .pixels
    .par_chunks_mut(frame.stride)
    .enumerate()
    .skip(bounds.1)
    .take(bounds.3 - bounds.1)
    .for_each(|(destination_y, row)| {
      for destination_x in bounds.0..bounds.2 {
        let mut alpha = 0.0;
        let mut color = [0.0; 3];
        for (index, weight) in weights.iter().enumerate() {
          let progress = index as f64 / (sample_count - 1) as f64;
          let exposure_offset = (progress - 0.8) * distance;
          let source = cursor.sample(
            destination_x as f64 + 0.5,
            destination_y as f64 + 0.5,
            x + direction_x * exposure_offset,
            y + direction_y * exposure_offset,
          );
          let sample_alpha = source[3] / 255.0;
          alpha += sample_alpha * weight;
          for channel in 0..3 {
            color[channel] += source[channel] * sample_alpha * weight;
          }
        }
        alpha /= total_weight;
        if alpha > 0.0 {
          for channel in &mut color {
            *channel /= total_weight * alpha;
          }
          blend_row(row, destination_x, [color[0], color[1], color[2], alpha]);
        }
      }
    });
}

fn bounds(
  frame: &FrameMut<'_>,
  x: f64,
  y: f64,
  radius: f64,
  blur: f64,
) -> (usize, usize, usize, usize) {
  (
    (x - radius - blur).floor().max(0.0) as usize,
    (y - radius - blur).floor().max(0.0) as usize,
    (x + radius + blur).ceil().min(frame.width as f64) as usize,
    (y + radius + blur).ceil().min(frame.height as f64) as usize,
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn interpolated_artwork_produces_fractional_edge_coverage() {
    let cursor = CursorRaster::new(CursorStyle::Arrow, 17.0, 28.0, 40.0, 0.0, 0.0, 4.0);
    let has_partial_pixel = (0..200).any(|y| {
      (0..200).any(|x| {
        let alpha = cursor.sample_for_draw(x as f64 + 0.5, y as f64 + 0.5, 40.0, 40.0)[3];
        alpha > 0.0 && alpha < 255.0
      })
    });
    assert!(has_partial_pixel);
  }

  #[test]
  fn fallback_arrow_keeps_its_native_aspect_inside_a_square_cursor_box() {
    let cursor = CursorRaster::new(CursorStyle::Arrow, 0.0, 32.0, 32.0, 0.0, 0.0, 1.0);
    let rightmost = (0..32)
      .flat_map(|y| (0..32).map(move |x| (x, y)))
      .filter(|(x, y)| cursor.sample(*x as f64 + 0.5, *y as f64 + 0.5, 0.0, 0.0)[3] > 0.0)
      .map(|(x, _)| x)
      .max()
      .unwrap();

    assert!(
      rightmost <= 23,
      "the 28:40 arrow was stretched to x={rightmost}"
    );
  }

  #[test]
  fn custom_cursors_never_share_a_system_style_artwork() {
    assert!(!uses_same_artwork(CursorStyle::Custom, CursorStyle::Arrow));
    assert!(uses_same_artwork(CursorStyle::Custom, CursorStyle::Custom));
    assert!(uses_same_artwork(CursorStyle::Arrow, CursorStyle::Arrow));
  }

  /// The test process never calls `initialize_system_artwork`, so
  /// `gpu_artworks` can only observe the fallback branch; the routing is
  /// pinned through `custom_gpu_artwork` with a stand-in arrow entry.
  #[cfg(target_os = "macos")]
  #[test]
  fn custom_cursors_index_the_system_arrow_fitted_to_their_box() {
    assert_eq!(
      artwork_index(CursorStyle::Custom),
      GPU_CUSTOM_ARTWORK_INDEX,
      "a custom cursor must not index the stretched system arrow"
    );
    let arrow = platform::StyleArtwork {
      hotspot_x: 5.0,
      hotspot_y: 5.0,
      image: RgbaImage::from_pixel(28, 40, image::Rgba([1, 2, 3, 255])),
    };
    let artwork = custom_gpu_artwork(Some(&arrow));
    assert_eq!(
      artwork.pixels,
      *arrow.image.as_raw(),
      "the custom slot must carry the system arrow's pixels"
    );
    assert!(
      artwork.use_design,
      "the custom slot must fit its design frame into the box, not stretch"
    );
    assert_eq!(artwork.design_width, 28.0);
    assert_eq!(artwork.design_height, 40.0);
    assert_eq!(
      (artwork.origin_x, artwork.origin_y),
      (5.0, 5.0),
      "the arrow is anchored by its own hotspot, not the recorded one"
    );
    assert!(
      !artwork.clip_local_box,
      "the fitted arrow reaches outside the recorded box at its hotspot"
    );
    assert!(!artwork.vertical);
    assert_eq!(
      artwork.pixels.len(),
      artwork.width as usize * artwork.height as usize * 4
    );
  }

  /// Without system artwork at all the slot keeps the baked vector arrow.
  #[cfg(target_os = "macos")]
  #[test]
  fn custom_cursors_fall_back_to_the_baked_arrow_without_system_artwork() {
    let artwork = custom_gpu_artwork(None);
    assert!(artwork.use_design);
    assert!(!artwork.clip_local_box);
    assert!(!artwork.vertical);
    assert_eq!(artwork.design_width, 28.0);
    assert_eq!(artwork.design_height, 40.0);
    assert_eq!(
      artwork.pixels.len(),
      artwork.width as usize * artwork.height as usize * 4
    );
    let artworks = gpu_artworks();
    let uploaded = artworks
      .get(GPU_CUSTOM_ARTWORK_INDEX as usize)
      .expect("the custom slot is uploaded");
    assert_eq!(uploaded.pixels, artwork.pixels);
  }

  /// The CPU preview has to place the system arrow by the same two rules the
  /// GPU slot carries: one aspect-preserving scale into the recorded box, and
  /// the arrow's own hotspot at the recorded position.
  #[test]
  fn custom_cursors_fit_the_system_arrow_by_its_own_hotspot() {
    let image: &'static RgbaImage = Box::leak(Box::new(RgbaImage::from_pixel(
      4,
      8,
      image::Rgba([255, 255, 255, 255]),
    )));
    let mut cursor = CursorRaster::new(CursorStyle::Custom, 0.0, 32.0, 32.0, 0.0, 0.0, 1.0);
    cursor.system_artwork = Some(image);
    cursor.system_design = Some(SystemDesign {
      height: 8.0,
      origin_x: 1.0,
      origin_y: 2.0,
      width: 4.0,
    });

    // min(32 / 4, 32 / 8) = 4, so the 4x8 arrow draws 16x32 and never fills
    // the square box's width.
    let lit = |x: f64, y: f64| cursor.sample(x, y, 0.0, 0.0)[3] > 0.0;
    assert!(lit(-3.5, -7.5), "the artwork's top-left corner was clipped");
    assert!(
      lit(11.5, 23.5),
      "the artwork's bottom-right corner is drawn"
    );
    assert!(
      !lit(12.5, 0.0),
      "the artwork was stretched past 4 x 4 units"
    );
    assert!(
      !lit(0.0, 24.5),
      "the artwork was stretched past 8 x 4 units"
    );
    assert!(
      !lit(-4.5, 0.0) && !lit(0.0, -8.5),
      "the artwork drew outside its design frame"
    );
  }

  /// With no system artwork loaded (as in this process) a custom cursor still
  /// draws the baked vector arrow at its own aspect and tip.
  #[test]
  fn custom_cursors_keep_the_fallback_arrows_aspect_in_a_square_box() {
    let cursor = CursorRaster::new(CursorStyle::Custom, 0.0, 32.0, 32.0, 0.0, 0.0, 1.0);
    let rightmost = (0..32)
      .flat_map(|y| (0..32).map(move |x| (x, y)))
      .filter(|(x, y)| cursor.sample(*x as f64 + 0.5, *y as f64 + 0.5, 0.0, 0.0)[3] > 0.0)
      .map(|(x, _)| x)
      .max()
      .unwrap();

    assert!(
      rightmost <= 23,
      "the custom cursor stretched the arrow to x={rightmost}"
    );
    assert!(
      cursor.sample(0.0, 0.0, 0.0, 0.0)[3] > 0.0,
      "the arrow's tip must sit at the drawn position"
    );
  }

  #[test]
  fn fallback_arrow_places_its_visible_tip_at_the_recorded_hotspot() {
    let cursor = CursorRaster::new(CursorStyle::Arrow, 0.0, 32.0, 32.0, 0.0, 0.0, 1.0);
    assert!(cursor.sample(0.0, 0.0, 0.0, 0.0)[3] > 0.0);
    assert!(
      cursor.sample(-0.5, 0.0, 0.0, 0.0)[3] > 0.0,
      "the rounded tip stroke was clipped at the hotspot"
    );
  }
}
