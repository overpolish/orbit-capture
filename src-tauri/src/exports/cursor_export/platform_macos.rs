// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
  ffi::{c_char, c_void, CString, OsString},
  path::PathBuf,
  sync::atomic::{AtomicU64, Ordering},
};

use super::*;

const LAYER_PROGRESS_PERCENT: u64 = 10;
const GPU_PROGRESS_PERCENT: u64 = 85;
static GPU_EXPORT_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

#[repr(C)]
struct GpuCallbacks<'a> {
  cancelled: &'a std::sync::atomic::AtomicBool,
  duration_ms: u64,
  on_progress: &'a mut dyn FnMut(u64),
}

#[repr(C)]
#[derive(Default)]
struct GpuCameraOverlay {
  crop_x: u32,
  crop_y: u32,
  crop_width: u32,
  crop_height: u32,
  frame_x: i32,
  frame_y: i32,
  frame_width: u32,
  frame_height: u32,
  radius: u32,
  drop_shadow: u32,
  camera_on_top: u32,
}

#[repr(C)]
#[derive(Default)]
struct GpuCanvas {
  background_color: [f32; 4],
  background_radius: u32,
  crop_x: i32,
  crop_y: i32,
  crop_width: u32,
  crop_height: u32,
  image_x: f32,
  image_y: f32,
  image_width: u32,
  image_height: u32,
  radius: u32,
  drop_shadow: u32,
  mesh_enabled: u32,
  mesh_seed: u32,
  mesh_warp_percent: f32,
  mesh_point_count: u32,
  mesh_points: [[f32; 8]; 4],
  mesh_colors: [[f32; 4]; 5],
  clip_cursor_at_video_edge: u32,
  transparent_background: u32,
}

unsafe extern "C" fn gpu_should_cancel(context: *mut c_void) -> bool {
  let callbacks = unsafe { &*(context.cast::<GpuCallbacks<'_>>()) };
  callbacks.cancelled.load(Ordering::Acquire)
}

unsafe extern "C" fn gpu_progress(context: *mut c_void, position_ms: u64) {
  let callbacks = unsafe { &mut *(context.cast::<GpuCallbacks<'_>>()) };
  let position_ms = position_ms.min(callbacks.duration_ms);
  (callbacks.on_progress)(
    callbacks.duration_ms.saturating_mul(LAYER_PROGRESS_PERCENT) / 100
      + position_ms.saturating_mul(GPU_PROGRESS_PERCENT) / 100,
  );
}

unsafe extern "C" {
  fn screenwide_gpu_composite_cursor(
    screen_path: *const c_char,
    cursor_path: *const c_char,
    commands_path: *const c_char,
    camera_path: *const c_char,
    camera_overlay: *const GpuCameraOverlay,
    canvas: *const GpuCanvas,
    output_path: *const c_char,
    source_width: u32,
    source_height: u32,
    width: u32,
    height: u32,
    bitrate: u64,
    context: *mut c_void,
    should_cancel: unsafe extern "C" fn(*mut c_void) -> bool,
    progress: unsafe extern "C" fn(*mut c_void, u64),
    error_text: *mut c_char,
    error_capacity: usize,
  ) -> i32;
}

fn c_path(path: &Path) -> Result<CString, String> {
  CString::new(path.as_os_str().as_encoded_bytes())
    .map_err(|_| format!("{} contains a null byte", path.display()))
}

fn gpu_video_path() -> PathBuf {
  let attempt = GPU_EXPORT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
  std::env::temp_dir().join(format!(
    "{}gpu-video-{}-{attempt}.mp4",
    media_preview::PREVIEW_PREFIX,
    std::process::id()
  ))
}

fn render_gpu_video(
  request: &mut CursorExportRequest<'_>,
  layer: Option<&native_macos::CursorLayer>,
  path: &Path,
) -> Result<ExportRunResult, String> {
  crate::screenshots::validate_output_settings(request.width, request.height, request.output)?;
  let screen = c_path(request.screen)?;
  let cursor = layer.map(|layer| c_path(&layer.movie)).transpose()?;
  let commands = layer.map(|layer| c_path(&layer.commands)).transpose()?;
  let camera = request.camera.map(|(path, _)| c_path(path)).transpose()?;
  let camera_overlay = request
    .camera
    .map(|(_, options)| media_preview::bake_geometry(options))
    .transpose()?
    .map(|geometry| {
      let scale_x = f64::from(request.output.width) / f64::from(geometry.output_width.max(1));
      let scale_y = f64::from(request.output.height) / f64::from(geometry.output_height.max(1));
      let scaled = |value: u32, scale: f64| (f64::from(value) * scale).round() as u32;
      let scaled_position = |value: i32, scale: f64| (f64::from(value) * scale).round() as i32;
      GpuCameraOverlay {
        crop_x: geometry.crop_x,
        crop_y: geometry.crop_y,
        crop_width: geometry.crop_width,
        crop_height: geometry.crop_height,
        frame_x: scaled_position(geometry.frame_x, scale_x),
        frame_y: scaled_position(geometry.frame_y, scale_y),
        frame_width: scaled(geometry.frame_width, scale_x),
        frame_height: scaled(geometry.frame_height, scale_y),
        radius: scaled(geometry.radius, scale_x.min(scale_y)),
        drop_shadow: u32::from(
          request
            .camera
            .is_some_and(|(_, options)| options.camera_drop_shadow),
        ),
        camera_on_top: u32::from(request.camera_on_top),
      }
    });
  let output = c_path(path)?;
  let placement =
    crate::screenshots::output_placement(request.width, request.height, request.output)?;
  let colour = crate::screenshots::parse_hex_colour(&request.output.background_color)?;
  let channel = |value: u8| f32::from(value) / 255.0;
  let mut canvas = GpuCanvas {
    background_color: [
      channel(colour[0]),
      channel(colour[1]),
      channel(colour[2]),
      1.0,
    ],
    background_radius: (f64::from(request.output.width.min(request.output.height))
      * request.output.background_radius_percent
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
    radius: (f64::from(placement.crop_width.min(placement.crop_height))
      * request.output.radius_percent
      / 100.0)
      .round() as u32,
    drop_shadow: u32::from(request.output.drop_shadow),
    mesh_enabled: u32::from(request.output.background_type == "mesh"),
    mesh_seed: request.output.mesh_seed,
    mesh_warp_percent: request.output.mesh_warp_percent as f32,
    mesh_point_count: request.output.mesh_points.len() as u32,
    clip_cursor_at_video_edge: u32::from(request.cursor_effects.clip_at_video_edge),
    transparent_background: 0,
    ..Default::default()
  };
  for (index, point) in request.output.mesh_points.iter().take(4).enumerate() {
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
  for (index, value) in request.output.mesh_colors.iter().take(5).enumerate() {
    let colour = crate::screenshots::parse_hex_colour(value)?;
    canvas.mesh_colors[index] = [
      channel(colour[0]),
      channel(colour[1]),
      channel(colour[2]),
      1.0,
    ];
  }
  let mut error = vec![0_i8; 2_048];
  let mut callbacks = GpuCallbacks {
    cancelled: request.cancelled,
    duration_ms: request.duration_ms,
    on_progress: request.on_progress,
  };
  let result = unsafe {
    screenwide_gpu_composite_cursor(
      screen.as_ptr(),
      cursor
        .as_ref()
        .map_or(std::ptr::null(), |path| path.as_ptr()),
      commands
        .as_ref()
        .map_or(std::ptr::null(), |path| path.as_ptr()),
      camera
        .as_ref()
        .map_or(std::ptr::null(), |path| path.as_ptr()),
      camera_overlay
        .as_ref()
        .map_or(std::ptr::null(), std::ptr::from_ref),
      &canvas,
      output.as_ptr(),
      request.width,
      request.height,
      request.output.width,
      request.output.height,
      super::video_bitrate(
        request.output.width,
        request.output.height,
        request.video.compression,
      ),
      (&mut callbacks as *mut GpuCallbacks<'_>).cast(),
      gpu_should_cancel,
      gpu_progress,
      error.as_mut_ptr(),
      error.len(),
    )
  };
  match result {
    1 => Ok(ExportRunResult::Completed),
    -1 => Ok(ExportRunResult::Cancelled),
    _ => {
      let message = unsafe { std::ffi::CStr::from_ptr(error.as_ptr()) }
        .to_string_lossy()
        .into_owned();
      Err(if message.is_empty() {
        "The Metal cursor compositor failed".to_owned()
      } else {
        message
      })
    }
  }
}

fn mux_gpu_video_args(
  request: &CursorExportRequest<'_>,
  video: &Path,
  temporary: &Path,
) -> Vec<OsString> {
  let mut args = ["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"]
    .map(OsString::from)
    .to_vec();
  args.push(video.into());
  args.extend([
    OsString::from("-i"),
    request.audio_source.unwrap_or(request.screen).into(),
  ]);
  args.extend(
    [
      "-progress",
      "pipe:1",
      "-nostats",
      "-map",
      "0:v:0",
      "-c:v",
      "copy",
    ]
    .map(OsString::from),
  );
  args.extend(
    request
      .selection
      .audio_args_from(request.audio_layout, 1)
      .into_iter()
      .map(OsString::from),
  );
  args.extend(
    [
      "-tag:v",
      "avc1",
      "-movflags",
      "+faststart",
      "-map_metadata",
      "-1",
      "-f",
      "mp4",
    ]
    .map(OsString::from),
  );
  args.push(temporary.into());
  args
}

fn export_gpu(mut request: CursorExportRequest<'_>) -> Result<ExportRunResult, String> {
  let (layer_result, layer) = native_macos::render(&mut request)?;
  if !matches!(layer_result, ExportRunResult::Completed) {
    return Ok(layer_result);
  }
  let result = (|| {
    let video = gpu_video_path();
    let video_result = render_gpu_video(&mut request, layer.as_ref(), &video)?;
    if !matches!(video_result, ExportRunResult::Completed) {
      let _ = std::fs::remove_file(&video);
      return Ok(video_result);
    }
    let temporary = media_preview::remux_temp_path(request.destination);
    let args = mux_gpu_video_args(&request, &video, &temporary);
    let duration_ms = request.duration_ms;
    let on_progress = &mut request.on_progress;
    let mut final_progress = |processed_ms: u64| {
      let remaining = 100 - LAYER_PROGRESS_PERCENT - GPU_PROGRESS_PERCENT;
      on_progress(
        duration_ms.saturating_mul(LAYER_PROGRESS_PERCENT + GPU_PROGRESS_PERCENT) / 100
          + processed_ms.saturating_mul(remaining) / 100,
      );
    };
    let result = media_preview::run_export(
      args,
      &temporary,
      request.destination,
      request.cancelled,
      &mut final_progress,
    );
    let _ = std::fs::remove_file(&video);
    result
  })();
  if let Some(layer) = layer {
    layer.remove();
  }
  if request.cancelled.load(Ordering::Acquire) {
    return Ok(ExportRunResult::Cancelled);
  }
  result
}

pub(super) fn export(request: CursorExportRequest<'_>) -> Result<ExportRunResult, String> {
  export_gpu(request)
}

#[cfg(test)]
#[path = "platform_macos_tests.rs"]
mod tests;
