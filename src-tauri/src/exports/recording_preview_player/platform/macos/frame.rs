// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use tauri::ipc::{Channel, InvokeResponseBody};

const NATIVE_FRAME_MARKER: u32 = u32::from_le_bytes(*b"OCPF");
const NATIVE_FRAME_VERSION: u32 = 2;

pub(crate) struct CursorPreview {
  pub canvas_height: u32,
  pub canvas_width: u32,
  pub pixels: Vec<u8>,
  pub x: i32,
  pub y: i32,
}

pub(super) fn send_frame(
  channel: &Channel,
  request_id: u64,
  screen: &[u8],
  camera: Option<&[u8]>,
  cursor: Option<&CursorPreview>,
) -> bool {
  let camera = camera.unwrap_or_default();
  let cursor_pixels = cursor.map_or(&[][..], |cursor| cursor.pixels.as_slice());
  let Ok(screen_len) = u32::try_from(screen.len()) else {
    return false;
  };
  let Ok(camera_len) = u32::try_from(camera.len()) else {
    return false;
  };
  let Ok(cursor_len) = u32::try_from(cursor_pixels.len()) else {
    return false;
  };
  let mut payload = Vec::with_capacity(44 + screen.len() + camera.len() + cursor_pixels.len());
  payload.extend_from_slice(&NATIVE_FRAME_MARKER.to_le_bytes());
  payload.extend_from_slice(&NATIVE_FRAME_VERSION.to_le_bytes());
  payload.extend_from_slice(&request_id.to_le_bytes());
  payload.extend_from_slice(&screen_len.to_le_bytes());
  payload.extend_from_slice(&camera_len.to_le_bytes());
  payload.extend_from_slice(&cursor_len.to_le_bytes());
  payload.extend_from_slice(&cursor.map_or(0, |cursor| cursor.x).to_le_bytes());
  payload.extend_from_slice(&cursor.map_or(0, |cursor| cursor.y).to_le_bytes());
  payload.extend_from_slice(&cursor.map_or(0, |cursor| cursor.canvas_width).to_le_bytes());
  payload.extend_from_slice(
    &cursor
      .map_or(0, |cursor| cursor.canvas_height)
      .to_le_bytes(),
  );
  payload.extend_from_slice(screen);
  payload.extend_from_slice(camera);
  payload.extend_from_slice(cursor_pixels);
  channel.send(InvokeResponseBody::Raw(payload)).is_ok()
}
