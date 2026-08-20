// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::super::ScreenshotWorkspaceOutputSettings;
use crate::screenshots::ScreenshotOutputSettings;

const MINIMUM_CANVAS_SIZE: f64 = 64.0;

pub(super) fn fit_workspace_to_items(
  snapshot: &ScreenshotWorkspaceOutputSettings,
  moved_index: usize,
  moved_output: &ScreenshotOutputSettings,
) -> ScreenshotWorkspaceOutputSettings {
  let width = f64::from(snapshot.canvas.width.max(1));
  let height = f64::from(snapshot.canvas.height.max(1));
  let mut next = snapshot.clone();
  if let Some(item) = next.items.get_mut(moved_index) {
    item.output = moved_output.clone();
  }
  let mut left = 0.0_f64;
  let mut top = 0.0_f64;
  let mut right = width;
  let mut bottom = height;
  for item in &next.items {
    let output = &item.output;
    let crop_x = width * output.screenshot_crop_x_percent / 100.0;
    let crop_y = height * output.screenshot_crop_y_percent / 100.0;
    let crop_width = width * output.screenshot_crop_width_percent / 100.0;
    let crop_height = height * output.screenshot_crop_height_percent / 100.0;
    left = left.min(crop_x.floor());
    top = top.min(crop_y.floor());
    right = right.max((crop_x + crop_width).ceil());
    bottom = bottom.max((crop_y + crop_height).ceil());
  }
  let next_width = (right - left).round().max(MINIMUM_CANVAS_SIZE);
  let next_height = (bottom - top).round().max(MINIMUM_CANVAS_SIZE);
  next.canvas.width = next_width as u32;
  next.canvas.height = next_height as u32;
  for item in &mut next.items {
    let output = &mut item.output;
    let crop_x = width * output.screenshot_crop_x_percent / 100.0 - left;
    let crop_y = height * output.screenshot_crop_y_percent / 100.0 - top;
    let crop_width = width * output.screenshot_crop_width_percent / 100.0;
    let crop_height = height * output.screenshot_crop_height_percent / 100.0;
    let image_width = width * output.screenshot_image_width_percent / 100.0;
    let image_x = width * output.screenshot_image_x_percent / 100.0 - left;
    let image_y = height * output.screenshot_image_y_percent / 100.0 - top;
    output.width = next_width as u32;
    output.height = next_height as u32;
    output.screenshot_crop_x_percent = crop_x * 100.0 / next_width;
    output.screenshot_crop_y_percent = crop_y * 100.0 / next_height;
    output.screenshot_crop_width_percent = crop_width * 100.0 / next_width;
    output.screenshot_crop_height_percent = crop_height * 100.0 / next_height;
    output.screenshot_image_width_percent = image_width * 100.0 / next_width;
    output.screenshot_image_x_percent = image_x * 100.0 / next_width;
    output.screenshot_image_y_percent = image_y * 100.0 / next_height;
  }
  next
}
