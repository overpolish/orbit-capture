// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! UIED-style element detection over a frozen monitor screenshot: gradient map
//! -> threshold binarization -> 3x3 morphological close -> connected components
//! -> element bounding boxes.
//!
//! This runs on the CPU with rayon. If the per-monitor gradient pass ever shows
//! up in profiles on 5K displays, the wgpu compute singleton in
//! `crate::screenshots::mesh_gpu` is the intended offload hook; it is
//! deliberately unused here because the cost is paid once per freeze and the
//! connected-component labeling that dominates the rest is CPU-bound anyway.

use rayon::prelude::*;
use serde::Serialize;
use std::{cmp::Reverse, collections::HashMap};

/// Per-axis neighbour deltas. `gx[y * width + x]` is the largest absolute
/// per-channel difference between pixel `x` and pixel `x - 1` on the same row
/// (column 0 is always 0); `gy` is the same against the row above (row 0 is
/// always 0).
///
/// An element covering columns `L..=R` therefore peaks at `x == L` and at
/// `x == R + 1`, so a component bounding box is naturally half-open `[L, R + 1)`
/// and its reported width matches the element's true width.
pub struct GradientMaps {
  pub gx: Vec<u8>,
  pub gy: Vec<u8>,
  pub width: u32,
  pub height: u32,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentBox {
  pub x: u32,
  pub y: u32,
  pub width: u32,
  pub height: u32,
}

const MAX_BOXES: usize = 8192;
const EDGE_SLACK: u32 = 2;
const BUCKET: u32 = 4;

fn channel_delta(a: &[u8], b: &[u8]) -> u8 {
  let red = a[0].abs_diff(b[0]);
  let green = a[1].abs_diff(b[1]);
  let blue = a[2].abs_diff(b[2]);
  red.max(green).max(blue)
}

pub fn compute_gradients(rgba: &[u8], width: u32, height: u32) -> GradientMaps {
  let (w, h) = (width as usize, height as usize);
  let mut gx = vec![0u8; w * h];
  let mut gy = vec![0u8; w * h];
  if w > 0 && h > 0 && rgba.len() >= w * h * 4 {
    gx.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
      let base = y * w * 4;
      for x in 1..w {
        row[x] = channel_delta(&rgba[base + (x - 1) * 4..], &rgba[base + x * 4..]);
      }
    });
    gy.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
      if y == 0 {
        return;
      }
      let (above, base) = ((y - 1) * w * 4, y * w * 4);
      for x in 0..w {
        row[x] = channel_delta(&rgba[above + x * 4..], &rgba[base + x * 4..]);
      }
    });
  }
  GradientMaps {
    gx,
    gy,
    width,
    height,
  }
}

/// 3x3 dilation, applied separably (horizontal pass then vertical pass).
fn dilate(source: &[bool], w: usize, h: usize) -> Vec<bool> {
  let mut middle = vec![false; w * h];
  for y in 0..h {
    let row = y * w;
    for x in 0..w {
      let left = x > 0 && source[row + x - 1];
      let right = x + 1 < w && source[row + x + 1];
      middle[row + x] = left || source[row + x] || right;
    }
  }
  let mut out = vec![false; w * h];
  for y in 0..h {
    for x in 0..w {
      let up = y > 0 && middle[(y - 1) * w + x];
      let down = y + 1 < h && middle[(y + 1) * w + x];
      out[y * w + x] = up || middle[y * w + x] || down;
    }
  }
  out
}

/// 3x3 erosion. Out-of-bounds neighbours count as foreground so that closing
/// stays extensive and never trims a component that touches the frame edge.
fn erode(source: &[bool], w: usize, h: usize) -> Vec<bool> {
  let mut middle = vec![false; w * h];
  for y in 0..h {
    let row = y * w;
    for x in 0..w {
      let left = x == 0 || source[row + x - 1];
      let right = x + 1 >= w || source[row + x + 1];
      middle[row + x] = left && source[row + x] && right;
    }
  }
  let mut out = vec![false; w * h];
  for y in 0..h {
    for x in 0..w {
      let up = y == 0 || middle[(y - 1) * w + x];
      let down = y + 1 >= h || middle[(y + 1) * w + x];
      out[y * w + x] = up && middle[y * w + x] && down;
    }
  }
  out
}

struct Component {
  min_x: usize,
  min_y: usize,
  max_x: usize,
  max_y: usize,
  pixels: u32,
}

/// Scanline-seeded flood fill with 8-connectivity.
fn components(binary: &[bool], w: usize, h: usize) -> Vec<Component> {
  let mut visited = vec![false; w * h];
  let mut stack: Vec<usize> = Vec::new();
  let mut found = Vec::new();
  for start in 0..w * h {
    if !binary[start] || visited[start] {
      continue;
    }
    visited[start] = true;
    stack.push(start);
    let mut component = Component {
      min_x: start % w,
      min_y: start / w,
      max_x: start % w,
      max_y: start / w,
      pixels: 0,
    };
    while let Some(index) = stack.pop() {
      let (x, y) = (index % w, index / w);
      component.pixels += 1;
      component.min_x = component.min_x.min(x);
      component.max_x = component.max_x.max(x);
      component.min_y = component.min_y.min(y);
      component.max_y = component.max_y.max(y);
      for ny in y.saturating_sub(1)..=(y + 1).min(h - 1) {
        for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
          let neighbour = ny * w + nx;
          if binary[neighbour] && !visited[neighbour] {
            visited[neighbour] = true;
            stack.push(neighbour);
          }
        }
      }
    }
    found.push(component);
  }
  found
}

fn area(candidate: &ComponentBox) -> u64 {
  u64::from(candidate.width) * u64::from(candidate.height)
}

fn near(a: &ComponentBox, b: &ComponentBox) -> bool {
  a.x.abs_diff(b.x) <= EDGE_SLACK
    && a.y.abs_diff(b.y) <= EDGE_SLACK
    && (a.x + a.width).abs_diff(b.x + b.width) <= EDGE_SLACK
    && (a.y + a.height).abs_diff(b.y + b.height) <= EDGE_SLACK
}

/// Drops boxes whose four edges all sit within `EDGE_SLACK` of an already kept
/// box. Candidates arrive sorted by pixel count so the denser component wins.
/// Bucketing on the top-left corner keeps this linear instead of quadratic.
fn dedupe(candidates: Vec<ComponentBox>) -> Vec<ComponentBox> {
  let mut buckets: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
  let mut kept: Vec<ComponentBox> = Vec::new();
  for candidate in candidates {
    let (cell_x, cell_y) = (candidate.x / BUCKET, candidate.y / BUCKET);
    let duplicate = (cell_x.saturating_sub(1)..=cell_x + 1).any(|bx| {
      (cell_y.saturating_sub(1)..=cell_y + 1).any(|by| {
        buckets
          .get(&(bx, by))
          .is_some_and(|indices| indices.iter().any(|index| near(&kept[*index], &candidate)))
      })
    });
    if duplicate {
      continue;
    }
    buckets
      .entry((cell_x, cell_y))
      .or_default()
      .push(kept.len());
    kept.push(candidate);
  }
  kept
}

/// Edge mass along a row: `2 * g[i] + g[i - 1] + g[i + 1]`, i.e. twice the
/// per-pixel value plus its two horizontal neighbours, compared against twice
/// the threshold.
///
/// Anti-aliasing splits an edge's contrast across two or three pixels, so a
/// subtle edge (a #F8F9FA card on white peaks at 7, and AA halves that again)
/// never clears a per-pixel threshold. Summing the ramp recovers it while
/// leaving hard 1 px ridges at their true contrast. Neighbours are clamped to
/// the same row so the mass never wraps around the frame edge.
///
/// The caller additionally requires the pixel's own gradient to be non-zero.
/// Without that guard a hard edge would also light up the flat pixels on either
/// side of it purely on borrowed mass, growing every reported box by one pixel
/// per side; a pixel that contributes nothing to the edge is not part of it.
fn edge_mass_x(gx: &[u8], index: usize, w: usize) -> u16 {
  let x = index % w;
  let left = if x > 0 { u16::from(gx[index - 1]) } else { 0 };
  let right = if x + 1 < w {
    u16::from(gx[index + 1])
  } else {
    0
  };
  2 * u16::from(gx[index]) + left + right
}

/// Vertical counterpart of [`edge_mass_x`]: neighbours are the same column in
/// the rows above and below, clamped at the frame.
fn edge_mass_y(gy: &[u8], index: usize, w: usize, h: usize) -> u16 {
  let y = index / w;
  let up = if y > 0 { u16::from(gy[index - w]) } else { 0 };
  let down = if y + 1 < h {
    u16::from(gy[index + w])
  } else {
    0
  };
  2 * u16::from(gy[index]) + up + down
}

pub fn detect_boxes(maps: &GradientMaps, threshold: u8) -> Vec<ComponentBox> {
  let (w, h) = (maps.width as usize, maps.height as usize);
  if w == 0 || h == 0 {
    return Vec::new();
  }
  let threshold = threshold.max(1);
  let double = u16::from(threshold) * 2;
  let binary: Vec<bool> = (0..w * h)
    .map(|index| {
      (maps.gx[index] > 0 && edge_mass_x(&maps.gx, index, w) >= double)
        || (maps.gy[index] > 0 && edge_mass_y(&maps.gy, index, w, h) >= double)
    })
    .collect();
  let closed = erode(&dilate(&binary, w, h), w, h);

  let mut candidates: Vec<(u32, ComponentBox)> = components(&closed, w, h)
    .into_iter()
    .filter_map(|component| {
      let candidate = ComponentBox {
        x: component.min_x as u32,
        y: component.min_y as u32,
        width: (component.max_x - component.min_x) as u32,
        height: (component.max_y - component.min_y) as u32,
      };
      let too_small = candidate.width < 3 || candidate.height < 3 || area(&candidate) < 16;
      let whole_frame = u64::from(candidate.width) * 100 >= maps.width as u64 * 95
        && u64::from(candidate.height) * 100 >= maps.height as u64 * 95;
      (!too_small && !whole_frame).then_some((component.pixels, candidate))
    })
    .collect();

  candidates.sort_by_key(|(pixels, _)| Reverse(*pixels));
  let mut boxes = dedupe(candidates.into_iter().map(|(_, item)| item).collect());
  if boxes.len() > MAX_BOXES {
    boxes.sort_by_key(|candidate| Reverse(area(candidate)));
    boxes.truncate(MAX_BOXES);
  }
  boxes.sort_by_key(area);
  boxes
}

#[cfg(test)]
mod tests;
