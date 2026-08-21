// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

const SIZE: u32 = 120;

struct Canvas {
  rgba: Vec<u8>,
  width: u32,
  height: u32,
}

impl Canvas {
  fn new(width: u32, height: u32, level: u8) -> Self {
    let mut rgba = vec![level; (width * height * 4) as usize];
    for pixel in rgba.chunks_mut(4) {
      pixel[3] = 255;
    }
    Self {
      rgba,
      width,
      height,
    }
  }

  fn set_rgb(&mut self, x: u32, y: u32, rgb: [u8; 3]) {
    if x >= self.width || y >= self.height {
      return;
    }
    let base = ((y * self.width + x) * 4) as usize;
    self.rgba[base..base + 3].copy_from_slice(&rgb);
  }

  fn set(&mut self, x: u32, y: u32, level: u8) {
    self.set_rgb(x, y, [level; 3]);
  }

  fn fill_rgb(&mut self, x: u32, y: u32, width: u32, height: u32, rgb: [u8; 3]) {
    for row in y..y + height {
      for column in x..x + width {
        self.set_rgb(column, row, rgb);
      }
    }
  }

  fn fill(&mut self, x: u32, y: u32, width: u32, height: u32, level: u8) {
    self.fill_rgb(x, y, width, height, [level; 3]);
  }

  fn border_rgb(&mut self, x: u32, y: u32, width: u32, height: u32, thickness: u32, rgb: [u8; 3]) {
    self.fill_rgb(x, y, width, thickness, rgb);
    self.fill_rgb(x, y + height - thickness, width, thickness, rgb);
    self.fill_rgb(x, y, thickness, height, rgb);
    self.fill_rgb(x + width - thickness, y, thickness, height, rgb);
  }

  fn border(&mut self, x: u32, y: u32, width: u32, height: u32, thickness: u32, level: u8) {
    self.border_rgb(x, y, width, height, thickness, [level; 3]);
  }

  fn boxes(&self, threshold: u8) -> Vec<ComponentBox> {
    let maps = compute_gradients(&self.rgba, self.width, self.height);
    detect_boxes(&maps, threshold)
  }
}

fn assert_box(found: &ComponentBox, expected: (u32, u32, u32, u32), slack: u32) {
  let actual = (found.x, found.y, found.width, found.height);
  let edges = [
    (actual.0, expected.0),
    (actual.1, expected.1),
    (actual.0 + actual.2, expected.0 + expected.2),
    (actual.1 + actual.3, expected.1 + expected.3),
  ];
  for (left, right) in edges {
    assert!(
      left.abs_diff(right) <= slack,
      "box {actual:?} differs from {expected:?} by more than {slack} px"
    );
  }
}

#[test]
fn a_solid_rectangle_yields_exactly_its_own_bounds() {
  let mut canvas = Canvas::new(SIZE, SIZE, 30);
  canvas.fill(20, 25, 40, 30, 220);
  let boxes = canvas.boxes(30);
  assert_eq!(boxes.len(), 1, "unexpected boxes: {boxes:?}");
  assert_box(&boxes[0], (20, 25, 40, 30), 1);
}

#[test]
fn an_anti_aliased_rectangle_still_yields_one_box() {
  let mut canvas = Canvas::new(SIZE, SIZE, 30);
  canvas.fill(20, 25, 40, 30, 220);
  // Two-pixel linear ramp on every edge, outside the solid core.
  for step in 1..=2u32 {
    let level = 30 + (190 * (3 - step) / 3) as u8;
    canvas.border(20 - step, 25 - step, 40 + step * 2, 30 + step * 2, 1, level);
  }
  let boxes = canvas.boxes(30);
  assert_eq!(boxes.len(), 1, "unexpected boxes: {boxes:?}");
  assert_box(&boxes[0], (20, 25, 40, 30), 2);
}

#[test]
fn a_subtle_anti_aliased_card_is_detected_at_high_tolerance() {
  // #F8F9FA on #FFFFFF: a max-channel delta of 7, halved again by the 1 px
  // anti-aliased border. Only the edge-mass rule can see this.
  let mut canvas = Canvas::new(SIZE, SIZE, 0xFF);
  canvas.fill_rgb(30, 30, 60, 40, [0xF8, 0xF9, 0xFA]);
  let blend = [
    (0xF8 + 0xFFu32).div_ceil(2) as u8,
    (0xF9 + 0xFFu32).div_ceil(2) as u8,
    (0xFA + 0xFFu32).div_ceil(2) as u8,
  ];
  canvas.border_rgb(30, 30, 60, 40, 1, blend);
  let boxes = canvas.boxes(5);
  assert_eq!(boxes.len(), 1, "unexpected boxes: {boxes:?}");
  assert_box(&boxes[0], (30, 30, 60, 40), 2);
  assert!(
    canvas.boxes(24).is_empty(),
    "subtle card should stay below medium tolerance"
  );
}

#[test]
fn nested_elements_are_detected_separately() {
  let mut canvas = Canvas::new(SIZE, SIZE, 128);
  canvas.border(10, 10, 91, 91, 2, 0);
  canvas.fill(30, 30, 40, 40, 255);
  let boxes = canvas.boxes(30);
  let outer = boxes
    .iter()
    .find(|candidate| candidate.x <= 11)
    .unwrap_or_else(|| panic!("no outer box in {boxes:?}"));
  let inner = boxes
    .iter()
    .find(|candidate| candidate.x >= 29 && candidate.x <= 31)
    .unwrap_or_else(|| panic!("no inner box in {boxes:?}"));
  assert_box(outer, (10, 10, 91, 91), 2);
  assert_box(inner, (30, 30, 40, 40), 2);
}

#[test]
fn sub_threshold_speckle_yields_no_boxes() {
  let mut canvas = Canvas::new(SIZE, SIZE, 128);
  let mut seed = 0x1234_5678u32;
  for y in 0..SIZE {
    for x in 0..SIZE {
      seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
      let jitter = (seed >> 16) % 11;
      canvas.set(x, y, 123 + jitter as u8);
    }
  }
  let boxes = canvas.boxes(30);
  assert!(boxes.is_empty(), "unexpected boxes: {boxes:?}");
}

#[test]
fn a_slow_gradient_background_yields_no_boxes() {
  let mut canvas = Canvas::new(SIZE, SIZE, 0);
  for y in 0..SIZE {
    for x in 0..SIZE {
      canvas.set(x, y, (40 + x) as u8);
    }
  }
  let boxes = canvas.boxes(30);
  assert!(boxes.is_empty(), "unexpected boxes: {boxes:?}");
}

#[test]
fn a_hard_vertical_edge_peaks_on_the_first_pixel_of_the_new_run() {
  let mut canvas = Canvas::new(SIZE, SIZE, 10);
  canvas.fill(50, 0, SIZE - 50, SIZE, 200);
  let maps = compute_gradients(&canvas.rgba, canvas.width, canvas.height);
  let row = (SIZE / 2 * SIZE) as usize;
  assert_eq!(maps.gx[row + 50], 190);
  for x in 0..SIZE as usize {
    if x != 50 {
      assert_eq!(maps.gx[row + x], 0, "unexpected gradient at x = {x}");
    }
  }
  assert!(maps.gy[row..row + SIZE as usize].iter().all(|v| *v == 0));
}
