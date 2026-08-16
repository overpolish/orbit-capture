// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use cidre::{cv, sc};
use std::ffi::c_char;

use crate::capture_kit::{display_scale, monitor_geometry, windows_to_exclude};
use crate::screenshots::{
  output_placement, parse_hex_colour, physical_capture_rect, CapturedImage,
  ScreenshotOutputSettings, ScreenshotTarget,
};

#[repr(C)]
#[derive(Default)]
pub(crate) struct NativeCanvas {
  pub(crate) background_color: [f32; 4],
  pub(crate) background_radius: u32,
  pub(crate) crop_x: i32,
  pub(crate) crop_y: i32,
  pub(crate) crop_width: u32,
  pub(crate) crop_height: u32,
  pub(crate) image_x: f32,
  pub(crate) image_y: f32,
  pub(crate) image_width: u32,
  pub(crate) image_height: u32,
  pub(crate) radius: u32,
  pub(crate) drop_shadow: u32,
  pub(crate) mesh_enabled: u32,
  pub(crate) mesh_seed: u32,
  pub(crate) mesh_warp_percent: f32,
  pub(crate) mesh_point_count: u32,
  pub(crate) mesh_points: [[f32; 8]; 4],
  pub(crate) mesh_colors: [[f32; 4]; 5],
  pub(crate) clip_cursor_at_video_edge: u32,
  pub(crate) transparent_background: u32,
  pub(crate) foreground_only: u32,
}

#[repr(C)]
#[derive(Default)]
pub(crate) struct StillOverlay {
  pub cursor_x: i32,
  pub cursor_y: i32,
  pub cursor_width: u32,
  pub cursor_height: u32,
  pub cursor_source_width: u32,
  pub cursor_source_height: u32,
  pub camera_crop_x: u32,
  pub camera_crop_y: u32,
  pub camera_crop_width: u32,
  pub camera_crop_height: u32,
  pub camera_frame_x: i32,
  pub camera_frame_y: i32,
  pub camera_frame_width: u32,
  pub camera_frame_height: u32,
  pub camera_radius: u32,
  pub camera_source_width: u32,
  pub camera_source_height: u32,
  pub camera_drop_shadow: u32,
  pub camera_on_top: u32,
}

unsafe extern "C" {
  fn screenwide_gpu_composite_still(
    source_rgba: *const u8,
    source_width: u32,
    source_height: u32,
    canvas: *const NativeCanvas,
    output_width: u32,
    output_height: u32,
    seconds: f64,
    cursor_rgba: *const u8,
    camera_rgba: *const u8,
    overlay: *const StillOverlay,
    output_rgba: *mut u8,
    error_text: *mut c_char,
    error_capacity: usize,
  ) -> i32;
  fn screenwide_gpu_alpha_composite(
    base_rgba: *const u8,
    overlay_rgba: *const u8,
    width: u32,
    height: u32,
    output_rgba: *mut u8,
    error_text: *mut c_char,
    error_capacity: usize,
  ) -> i32;
}

pub(crate) fn native_canvas(
  source_width: u32,
  source_height: u32,
  settings: &ScreenshotOutputSettings,
  transparent_background: bool,
) -> Result<NativeCanvas, String> {
  super::validate_output_settings(source_width, source_height, settings)?;
  let placement = output_placement(source_width, source_height, settings)?;
  let colour = parse_hex_colour(&settings.background_color)?;
  let channel = |value: u8| f32::from(value) / 255.0;
  let mut canvas = NativeCanvas {
    background_color: [
      channel(colour[0]),
      channel(colour[1]),
      channel(colour[2]),
      1.0,
    ],
    background_radius: (f64::from(settings.width.min(settings.height))
      * settings.background_radius_percent
      / 100.0)
      .round() as u32,
    crop_x: placement.crop_x,
    crop_y: placement.crop_y,
    crop_width: placement.crop_width,
    crop_height: placement.crop_height,
    image_x: placement.image_x as f32,
    image_y: placement.image_y as f32,
    image_width: placement.image_width,
    image_height: placement.image_height,
    radius: (f64::from(placement.crop_width.min(placement.crop_height)) * settings.radius_percent
      / 100.0)
      .round() as u32,
    drop_shadow: u32::from(settings.drop_shadow),
    mesh_enabled: u32::from(settings.background_type == "mesh"),
    mesh_seed: settings.mesh_seed,
    mesh_warp_percent: settings.mesh_warp_percent as f32,
    mesh_point_count: settings.mesh_points.len() as u32,
    transparent_background: u32::from(transparent_background),
    ..Default::default()
  };
  for (index, point) in settings.mesh_points.iter().take(4).enumerate() {
    let angle = point.rotation.to_radians() as f32;
    canvas.mesh_points[index] = [
      point.x as f32 / 100.0,
      point.y as f32 / 100.0,
      point.radius_x as f32 / 100.0,
      point.radius_y as f32 / 100.0,
      angle.cos(),
      angle.sin(),
      0.0,
      0.0,
    ];
  }
  for (index, value) in settings.mesh_colors.iter().take(5).enumerate() {
    let colour = parse_hex_colour(value)?;
    canvas.mesh_colors[index] = [
      channel(colour[0]),
      channel(colour[1]),
      channel(colour[2]),
      1.0,
    ];
  }
  Ok(canvas)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn compose_output_layers(
  image: &CapturedImage,
  settings: &ScreenshotOutputSettings,
  seconds: f64,
  transparent_background: bool,
  cursor: Option<&CapturedImage>,
  camera: Option<&CapturedImage>,
  overlay: Option<&StillOverlay>,
  clip_cursor_at_video_edge: bool,
  foreground_only: bool,
) -> Result<CapturedImage, String> {
  let mut canvas = native_canvas(image.width, image.height, settings, transparent_background)?;
  canvas.clip_cursor_at_video_edge = u32::from(clip_cursor_at_video_edge);
  canvas.foreground_only = u32::from(foreground_only);
  let mut rgba = vec![0_u8; settings.width as usize * settings.height as usize * 4];
  let mut error = vec![0_i8; 2_048];
  let result = unsafe {
    screenwide_gpu_composite_still(
      image.rgba.as_ptr(),
      image.width,
      image.height,
      &canvas,
      settings.width,
      settings.height,
      seconds,
      cursor.map_or(std::ptr::null(), |image| image.rgba.as_ptr()),
      camera.map_or(std::ptr::null(), |image| image.rgba.as_ptr()),
      overlay.map_or(std::ptr::null(), std::ptr::from_ref),
      rgba.as_mut_ptr(),
      error.as_mut_ptr(),
      error.len(),
    )
  };
  if result == 0 {
    let message = unsafe { std::ffi::CStr::from_ptr(error.as_ptr()) }
      .to_string_lossy()
      .into_owned();
    return Err(if message.is_empty() {
      "The native screenshot compositor failed".to_owned()
    } else {
      message
    });
  }
  Ok(CapturedImage {
    height: settings.height,
    rgba,
    width: settings.width,
  })
}

pub(crate) fn alpha_composite(
  base: &CapturedImage,
  overlay: &CapturedImage,
) -> Result<CapturedImage, String> {
  if base.width != overlay.width || base.height != overlay.height {
    return Err("The screenshot layers do not share a canvas size".to_owned());
  }
  let mut rgba = vec![0_u8; base.rgba.len()];
  let mut error = vec![0_i8; 2_048];
  let result = unsafe {
    screenwide_gpu_alpha_composite(
      base.rgba.as_ptr(),
      overlay.rgba.as_ptr(),
      base.width,
      base.height,
      rgba.as_mut_ptr(),
      error.as_mut_ptr(),
      error.len(),
    )
  };
  if result == 0 {
    let message = unsafe { std::ffi::CStr::from_ptr(error.as_ptr()) }
      .to_string_lossy()
      .into_owned();
    return Err(if message.is_empty() {
      "The native screenshot layer compositor failed".to_owned()
    } else {
      message
    });
  }
  Ok(CapturedImage {
    height: base.height,
    rgba,
    width: base.width,
  })
}

async fn capture_filtered(
  filter: &sc::ContentFilter,
  cfg: &sc::StreamCfg,
) -> Result<CapturedImage, String> {
  let mut buf = sc::ScreenshotManager::capture_sample_buf(filter, cfg)
    .await
    .map_err(|error| error.to_string())?;
  let image = buf
    .image_buf_mut()
    .ok_or_else(|| "The capture produced no image".to_owned())?;
  let width = image.width();
  let height = image.height();
  let stride = image.bytes_per_row();

  if width == 0 || height == 0 {
    return Err("The capture produced an empty image".to_owned());
  }

  let flags = cv::pixel_buffer::LockFlags::READ_ONLY;
  // SAFETY: the buffer stays locked for exactly the copy below, and every read
  // is bounded by the stride and height the buffer itself reports.
  unsafe { image.lock_base_addr(flags) }
    .result()
    .map_err(|error| error.to_string())?;
  let base = unsafe { image.base_address() } as *const u8;
  if base.is_null() {
    unsafe { image.unlock_lock_base_addr(flags) };
    return Err("The capture produced no pixels".to_owned());
  }

  // ScreenCaptureKit hands back BGRA with rows padded out to its own stride,
  // while the clipboard and the PNG encoder both want packed RGBA.
  let mut rgba = vec![0_u8; width * height * 4];
  for row in 0..height {
    let source = unsafe { std::slice::from_raw_parts(base.add(row * stride), width * 4) };
    let target = &mut rgba[row * width * 4..(row + 1) * width * 4];
    for (source, target) in source.chunks_exact(4).zip(target.chunks_exact_mut(4)) {
      target[0] = source[2];
      target[1] = source[1];
      target[2] = source[0];
      target[3] = source[3];
    }
  }
  unsafe { image.unlock_lock_base_addr(flags) };

  Ok(CapturedImage {
    rgba,
    width: width as u32,
    height: height as u32,
  })
}

/// ScreenCaptureKit deals in Objective-C objects, which are not `Send`, so the
/// whole conversation is confined to one blocking thread and only the finished
/// pixels travel back out.
pub fn capture_blocking(
  target: ScreenshotTarget,
  include_own_windows: bool,
  show_cursor: bool,
) -> Result<CapturedImage, String> {
  tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .map_err(|error| error.to_string())?
    .block_on(capture(target, include_own_windows, show_cursor))
}

pub fn capture_for_text_recognition_blocking(
  target: ScreenshotTarget,
  excluded_window_ids: &[u32],
) -> Result<CapturedImage, String> {
  tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .map_err(|error| error.to_string())?
    .block_on(capture_for_text_recognition(target, excluded_window_ids))
}

async fn capture_for_text_recognition(
  target: ScreenshotTarget,
  excluded_window_ids: &[u32],
) -> Result<CapturedImage, String> {
  let ScreenshotTarget::Region { monitor_id, region } = target else {
    return Err("Text recognition requires a region".to_owned());
  };
  let content = sc::ShareableContent::current()
    .await
    .map_err(|error| error.to_string())?;
  let displays = content.displays();
  let display = displays
    .iter()
    .find(|display| display.display_id().0 == monitor_id)
    .ok_or_else(|| "The selected monitor is no longer available".to_owned())?;
  // Only the recognition overlays are hidden, whatever the "record Screenwide's
  // windows" setting says: their selection marquee sits over the very
  // text being read, so letting it into the image would corrupt the result.
  let excluded = content
    .windows()
    .iter()
    .filter(|window| excluded_window_ids.contains(&window.id()))
    .map(|window| window.retained())
    .collect::<Vec<_>>();
  let excluded = cidre::ns::Array::from_slice_retained(&excluded);
  let (scale, monitor_width, monitor_height) = monitor_geometry(monitor_id)?;
  let rect = physical_capture_rect(region, scale, monitor_width, monitor_height)
    .ok_or_else(|| "The selected region is not on the monitor".to_owned())?;
  let mut cfg = sc::StreamCfg::new();
  cfg.set_shows_cursor(false);
  cfg.set_pixel_format(cv::PixelFormat::_32_BGRA);
  cfg.set_src_rect(cidre::cg::Rect::new(
    f64::from(rect.x) / scale,
    f64::from(rect.y) / scale,
    f64::from(rect.width) / scale,
    f64::from(rect.height) / scale,
  ));
  cfg.set_width(rect.width as usize);
  cfg.set_height(rect.height as usize);
  let filter = sc::ContentFilter::with_display_excluding_windows(display, &excluded);
  capture_filtered(&filter, &cfg).await
}

async fn capture(
  target: ScreenshotTarget,
  include_own_windows: bool,
  show_cursor: bool,
) -> Result<CapturedImage, String> {
  let content = sc::ShareableContent::current()
    .await
    .map_err(|error| error.to_string())?;
  let mut cfg = sc::StreamCfg::new();
  cfg.set_shows_cursor(show_cursor);
  cfg.set_pixel_format(cv::PixelFormat::_32_BGRA);

  match target {
    ScreenshotTarget::Screen { monitor_id } => {
      let displays = content.displays();
      let display = displays
        .iter()
        .find(|display| display.display_id().0 == monitor_id)
        .ok_or_else(|| "The selected monitor is no longer available".to_owned())?;
      let (_, width, height) = monitor_geometry(monitor_id)?;
      cfg.set_width(width as usize);
      cfg.set_height(height as usize);

      let filter = sc::ContentFilter::with_display_excluding_windows(
        display,
        &windows_to_exclude(&content, include_own_windows),
      );
      capture_filtered(&filter, &cfg).await
    }
    ScreenshotTarget::Region { monitor_id, region } => {
      let displays = content.displays();
      let display = displays
        .iter()
        .find(|display| display.display_id().0 == monitor_id)
        .ok_or_else(|| "The selected monitor is no longer available".to_owned())?;
      let (scale, monitor_width, monitor_height) = monitor_geometry(monitor_id)?;
      let rect = physical_capture_rect(region, scale, monitor_width, monitor_height)
        .ok_or_else(|| "The selected region is not on the monitor".to_owned())?;

      // The source rect is in points, so the one physical rectangle both
      // platforms agree on is divided back down here - and only here.
      cfg.set_src_rect(cidre::cg::Rect::new(
        f64::from(rect.x) / scale,
        f64::from(rect.y) / scale,
        f64::from(rect.width) / scale,
        f64::from(rect.height) / scale,
      ));
      cfg.set_width(rect.width as usize);
      cfg.set_height(rect.height as usize);

      let filter = sc::ContentFilter::with_display_excluding_windows(
        display,
        &windows_to_exclude(&content, include_own_windows),
      );
      capture_filtered(&filter, &cfg).await
    }
    ScreenshotTarget::Window { window_id } => {
      let windows = content.windows();
      let window = windows
        .iter()
        .find(|window| window.id() == window_id)
        .ok_or_else(|| "The selected window is no longer available".to_owned())?;
      let frame = window.frame();
      let displays = content.displays();
      let scale = displays
        .iter()
        .find(|display| {
          let bounds = display.frame();
          let centre_x = frame.origin.x + frame.size.width / 2.0;
          let centre_y = frame.origin.y + frame.size.height / 2.0;
          centre_x >= bounds.origin.x
            && centre_x < bounds.origin.x + bounds.size.width
            && centre_y >= bounds.origin.y
            && centre_y < bounds.origin.y + bounds.size.height
        })
        .map_or(1.0, |display| display_scale(display.display_id().0));
      cfg.set_width((frame.size.width * scale).round() as usize);
      cfg.set_height((frame.size.height * scale).round() as usize);

      let filter = sc::ContentFilter::with_desktop_independent_window(window);
      capture_filtered(&filter, &cfg).await
    }
  }
}
