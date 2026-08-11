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
  frame_x: u32,
  frame_y: u32,
  frame_width: u32,
  frame_height: u32,
  radius: u32,
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
  fn orbit_gpu_composite_cursor(
    screen_path: *const c_char,
    cursor_path: *const c_char,
    commands_path: *const c_char,
    camera_path: *const c_char,
    camera_overlay: *const GpuCameraOverlay,
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
  layer: &native_macos::CursorLayer,
  path: &Path,
) -> Result<ExportRunResult, String> {
  let screen = c_path(request.screen)?;
  let cursor = c_path(&layer.movie)?;
  let commands = c_path(&layer.commands)?;
  let camera = request.camera.map(|(path, _)| c_path(path)).transpose()?;
  let camera_overlay = request
    .camera
    .map(|(_, options)| media_preview::bake_geometry(options))
    .transpose()?
    .map(|geometry| {
      if (geometry.output_width, geometry.output_height) != (layer.width, layer.height) {
        return Err("The camera and screen export sizes do not match".to_owned());
      }
      Ok(GpuCameraOverlay {
        crop_x: geometry.crop_x,
        crop_y: geometry.crop_y,
        crop_width: geometry.crop_width,
        crop_height: geometry.crop_height,
        frame_x: geometry.frame_x,
        frame_y: geometry.frame_y,
        frame_width: geometry.frame_width,
        frame_height: geometry.frame_height,
        radius: geometry.radius,
      })
    })
    .transpose()?;
  let output = c_path(path)?;
  let mut error = vec![0_i8; 2_048];
  let mut callbacks = GpuCallbacks {
    cancelled: request.cancelled,
    duration_ms: request.duration_ms,
    on_progress: request.on_progress,
  };
  let result = unsafe {
    orbit_gpu_composite_cursor(
      screen.as_ptr(),
      cursor.as_ptr(),
      commands.as_ptr(),
      camera
        .as_ref()
        .map_or(std::ptr::null(), |path| path.as_ptr()),
      camera_overlay
        .as_ref()
        .map_or(std::ptr::null(), std::ptr::from_ref),
      output.as_ptr(),
      request.width,
      request.height,
      layer.width,
      layer.height,
      super::video_bitrate(layer.width, layer.height, request.video.compression),
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
  args.extend([OsString::from("-i"), request.screen.into()]);
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
  let layer = layer.ok_or_else(|| "The cursor layer was not created".to_owned())?;
  let result = (|| {
    let video = gpu_video_path();
    let video_result = render_gpu_video(&mut request, &layer, &video)?;
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
  layer.remove();
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
