// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
  io::{Read, Write},
  path::PathBuf,
  process::{Command, Stdio},
  sync::atomic::{AtomicU64, Ordering},
};

use super::*;
use crate::exports::cursor_effects::{CursorCompositor, CursorOverlayCache};

const CURSOR_FRAME_RATE: u64 = 60;
const LAYER_PROGRESS_SHARE: u64 = 10;
static EXPORT_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

pub(super) struct CursorLayer {
  pub commands: PathBuf,
  pub movie: PathBuf,
}

impl CursorLayer {
  pub(super) fn remove(&self) {
    let _ = std::fs::remove_file(&self.movie);
    let _ = std::fs::remove_file(&self.commands);
  }
}

fn temporary_paths() -> (PathBuf, PathBuf) {
  let attempt = EXPORT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
  let stem = format!(
    "{}cursor-layer-{}-{attempt}",
    media_preview::PREVIEW_PREFIX,
    std::process::id()
  );
  let directory = std::env::temp_dir();
  (
    directory.join(format!("{stem}.mov")),
    directory.join(format!("{stem}.cmd")),
  )
}

pub(super) fn scaled_size(request: &CursorExportRequest<'_>) -> Result<(u32, u32), String> {
  let placement =
    crate::screenshots::output_placement(request.width, request.height, request.output)?;
  Ok((placement.image_width, placement.image_height))
}

fn finish_layer_encoder(
  mut child: std::process::Child,
  stderr_reader: std::thread::JoinHandle<Vec<u8>>,
  movie: &Path,
  cancelled: bool,
) -> Result<ExportRunResult, String> {
  if cancelled {
    let _ = child.kill();
  }
  let status = child.wait().map_err(|error| error.to_string())?;
  let stderr = stderr_reader.join().unwrap_or_default();
  if cancelled {
    let _ = std::fs::remove_file(movie);
    return Ok(ExportRunResult::Cancelled);
  }
  if !status.success() || std::fs::metadata(movie).map_or(true, |metadata| metadata.len() == 0) {
    let _ = std::fs::remove_file(movie);
    return Err(media_preview::remux_error(&stderr));
  }
  Ok(ExportRunResult::Completed)
}

pub(super) fn render(
  request: &mut CursorExportRequest<'_>,
) -> Result<(ExportRunResult, Option<CursorLayer>), String> {
  let Some(cursor_path) = request.cursor else {
    return Ok((ExportRunResult::Completed, None));
  };
  let cursor = CursorCompositor::open(cursor_path)?;
  let (output_width, output_height) = scaled_size(request)?;
  let placement =
    crate::screenshots::output_placement(request.width, request.height, request.output)?;
  let layer_size = cursor.overlay_size(
    output_width as usize,
    output_height as usize,
    request.cursor_effects,
  );
  let (movie, commands) = temporary_paths();
  let mut child = Command::new(media_preview::ffmpeg_path())
    .args([
      "-hide_banner",
      "-loglevel",
      "error",
      "-nostdin",
      "-y",
      "-f",
      "rawvideo",
      "-pixel_format",
      "bgra",
      "-video_size",
      &format!("{layer_size}x{layer_size}"),
      "-framerate",
      &CURSOR_FRAME_RATE.to_string(),
      "-i",
      "pipe:0",
      "-an",
      "-c:v",
      "prores_videotoolbox",
      "-allow_sw",
      "0",
      "-prio_speed",
      "1",
      "-profile:v",
      "4444",
      "-pix_fmt",
      "bgra",
    ])
    .arg(&movie)
    .stdin(Stdio::piped())
    .stdout(Stdio::null())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|error| format!("The cursor layer encoder could not be started: {error}"))?;
  let stderr = child
    .stderr
    .take()
    .ok_or_else(|| "The cursor layer encoder did not expose its errors".to_owned())?;
  let stderr_reader = std::thread::spawn(move || {
    let mut bytes = Vec::new();
    let _ = stderr.take(64 * 1024).read_to_end(&mut bytes);
    bytes
  });
  let mut stdin = child
    .stdin
    .take()
    .ok_or_else(|| "The cursor layer encoder did not accept frames".to_owned())?;
  let frame_count = request
    .duration_ms
    .saturating_mul(CURSOR_FRAME_RATE)
    .div_ceil(1_000)
    .saturating_add(1);
  let mut pixels = vec![0_u8; layer_size * layer_size * 4];
  let mut cache = CursorOverlayCache::new();
  let mut command_text = String::with_capacity(frame_count as usize * 72);
  let mut previous_position = None;
  let mut write_error = None;
  for frame in 0..frame_count {
    if request.cancelled.load(Ordering::Acquire) {
      break;
    }
    let position_ms = frame.saturating_mul(1_000) / CURSOR_FRAME_RATE;
    let frame_seconds = frame as f64 / CURSOR_FRAME_RATE as f64;
    let position = cursor.composite_overlay_bgra(
      &mut pixels,
      layer_size,
      (output_width as usize, output_height as usize),
      position_ms,
      request.cursor_effects,
      &mut cache,
    );
    if previous_position != Some(position) {
      let (x, y) = position.map_or((-100_000, -100_000), |position| {
        (
          position.x.saturating_add(placement.image_x.round() as i32),
          position.y.saturating_add(placement.image_y.round() as i32),
        )
      });
      command_text.push_str(&format!(
        "{:.9} overlay@cursor x {x}, overlay@cursor y {y};\n",
        frame_seconds
      ));
      previous_position = Some(position);
    }
    if let Err(error) = stdin.write_all(&pixels) {
      write_error = Some(error.to_string());
      break;
    }
    (request.on_progress)(
      position_ms
        .saturating_mul(LAYER_PROGRESS_SHARE)
        .checked_div(100)
        .unwrap_or(0),
    );
  }
  drop(stdin);
  let cancelled = request.cancelled.load(Ordering::Acquire);
  let result = finish_layer_encoder(child, stderr_reader, &movie, cancelled)?;
  if !matches!(result, ExportRunResult::Completed) {
    let _ = std::fs::remove_file(&commands);
    return Ok((result, None));
  }
  if let Some(error) = write_error {
    let _ = std::fs::remove_file(&movie);
    return Err(format!("The cursor layer could not be written: {error}"));
  }
  std::fs::write(&commands, command_text).map_err(|error| {
    let _ = std::fs::remove_file(&movie);
    format!("The cursor motion could not be written: {error}")
  })?;
  Ok((
    ExportRunResult::Completed,
    Some(CursorLayer { commands, movie }),
  ))
}
