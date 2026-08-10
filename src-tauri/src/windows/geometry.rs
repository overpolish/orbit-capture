// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use tauri::{AppHandle, Monitor, PhysicalPosition, PhysicalSize, WebviewWindow};

/// The area, in physical pixels, that a window shares with a monitor.
pub(super) fn overlap_area(
  monitor_position: PhysicalPosition<i32>,
  monitor_size: PhysicalSize<u32>,
  window_position: PhysicalPosition<i32>,
  window_size: PhysicalSize<u32>,
) -> i64 {
  let left = window_position.x.max(monitor_position.x);
  let top = window_position.y.max(monitor_position.y);
  let right = (window_position.x + window_size.width as i32)
    .min(monitor_position.x + monitor_size.width as i32);
  let bottom = (window_position.y + window_size.height as i32)
    .min(monitor_position.y + monitor_size.height as i32);

  i64::from((right - left).max(0)) * i64::from((bottom - top).max(0))
}

/// Whether any part of a window still lands on a connected monitor. A saved
/// position stops being usable the moment its display is unplugged or moved.
pub(super) fn window_is_on_a_monitor(
  app: &AppHandle,
  window: &WebviewWindow,
) -> tauri::Result<bool> {
  let window_position = window.outer_position()?;
  let window_size = window.outer_size()?;

  Ok(app.available_monitors()?.iter().any(|monitor| {
    overlap_area(
      *monitor.position(),
      *monitor.size(),
      window_position,
      window_size,
    ) > 0
  }))
}

/// The monitor a window sits on most, for containment purposes.
pub(super) fn monitor_with_most_overlap(
  app: &AppHandle,
  window: &WebviewWindow,
) -> tauri::Result<Option<Monitor>> {
  let window_position = window.outer_position()?;
  let window_size = window.outer_size()?;
  let monitors = app.available_monitors()?;
  let target = monitors
    .iter()
    .max_by_key(|monitor| {
      overlap_area(
        *monitor.position(),
        *monitor.size(),
        window_position,
        window_size,
      )
    })
    .or_else(|| monitors.first());

  Ok(target.cloned())
}

fn contained_position(
  area_position: PhysicalPosition<i32>,
  area_size: PhysicalSize<u32>,
  window_position: PhysicalPosition<i32>,
  window_size: PhysicalSize<u32>,
) -> PhysicalPosition<i32> {
  let maximum_x = area_position.x + area_size.width.saturating_sub(window_size.width) as i32;
  let maximum_y = area_position.y + area_size.height.saturating_sub(window_size.height) as i32;

  PhysicalPosition::new(
    window_position.x.clamp(area_position.x, maximum_x),
    window_position.y.clamp(area_position.y, maximum_y),
  )
}

fn contained_size(
  area_size: PhysicalSize<u32>,
  window_size: PhysicalSize<u32>,
) -> PhysicalSize<u32> {
  PhysicalSize::new(
    window_size.width.min(area_size.width),
    window_size.height.min(area_size.height),
  )
}

pub(super) fn contain_window_in_work_area(
  app: &AppHandle,
  window: &WebviewWindow,
) -> tauri::Result<()> {
  if window.is_fullscreen()? || window.is_maximized()? {
    return Ok(());
  }

  let Some(monitor) = monitor_with_most_overlap(app, window)? else {
    return Ok(());
  };
  let work_area = monitor.work_area();
  let position = window.outer_position()?;
  let size = window.outer_size()?;
  let contained_size = contained_size(work_area.size, size);
  let contained = contained_position(work_area.position, work_area.size, position, contained_size);

  if size != contained_size {
    window.set_size(contained_size)?;
  }
  if position != contained {
    window.set_position(contained)?;
  }

  Ok(())
}

pub(super) fn keep_window_on_a_monitor(
  app: &AppHandle,
  window: &WebviewWindow,
) -> tauri::Result<()> {
  let window_size = window.outer_size()?;

  if !window_is_on_a_monitor(app, window)? {
    if let Some(monitor) = app.primary_monitor()? {
      let position = monitor.position();
      let size = monitor.size();
      window.set_position(PhysicalPosition {
        x: position.x + (size.width.saturating_sub(window_size.width) / 2) as i32,
        y: position.y + size.height.saturating_sub(window_size.height + 100) as i32,
      })?;
    }
  }

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  const WORK_AREA_POSITION: PhysicalPosition<i32> = PhysicalPosition::new(100, 50);
  const WORK_AREA_SIZE: PhysicalSize<u32> = PhysicalSize::new(1_600, 900);
  const WINDOW_SIZE: PhysicalSize<u32> = PhysicalSize::new(800, 600);

  #[test]
  fn keeps_a_window_that_already_fits_unchanged() {
    let position = PhysicalPosition::new(500, 200);

    assert_eq!(
      contained_position(WORK_AREA_POSITION, WORK_AREA_SIZE, position, WINDOW_SIZE),
      position
    );
  }

  #[test]
  fn clamps_every_edge_to_the_work_area() {
    assert_eq!(
      contained_position(
        WORK_AREA_POSITION,
        WORK_AREA_SIZE,
        PhysicalPosition::new(-50, -50),
        WINDOW_SIZE,
      ),
      WORK_AREA_POSITION
    );
    assert_eq!(
      contained_position(
        WORK_AREA_POSITION,
        WORK_AREA_SIZE,
        PhysicalPosition::new(1_500, 800),
        WINDOW_SIZE,
      ),
      PhysicalPosition::new(900, 350)
    );
  }

  #[test]
  fn anchors_a_window_larger_than_the_work_area() {
    assert_eq!(
      contained_position(
        WORK_AREA_POSITION,
        WORK_AREA_SIZE,
        PhysicalPosition::new(500, 200),
        PhysicalSize::new(2_000, 1_000),
      ),
      WORK_AREA_POSITION
    );
  }

  #[test]
  fn reduces_an_oversized_window_to_the_work_area() {
    assert_eq!(
      contained_size(WORK_AREA_SIZE, PhysicalSize::new(2_000, 1_000)),
      WORK_AREA_SIZE
    );
    assert_eq!(contained_size(WORK_AREA_SIZE, WINDOW_SIZE), WINDOW_SIZE);
  }
}
