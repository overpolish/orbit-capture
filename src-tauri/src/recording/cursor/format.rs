// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde::{Deserialize, Serialize};

pub(crate) const FORMAT_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CursorStyle {
  Arrow,
  ClosedHand,
  ContextMenu,
  Crosshair,
  Custom,
  DisappearingItem,
  DragCopy,
  DragLink,
  IBeam,
  NotAllowed,
  OpenHand,
  PointingHand,
  ResizeHorizontal,
  ResizeVertical,
  VerticalIBeam,
  ZoomIn,
  ZoomOut,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CursorButton {
  Left,
  Middle,
  Right,
  Other(u8),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ButtonState {
  Down,
  Up,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CursorSourceKind {
  Region,
  Screen,
  Window,
}

/// The relationship between global pointer coordinates and the recorded
/// pixels at time zero. Bounds are global logical points; video dimensions are
/// the encoded pixel dimensions. A later format version can add timestamped
/// bounds records for moving windows without changing pointer events.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorSource {
  pub height: f64,
  pub kind: CursorSourceKind,
  pub platform_id: String,
  pub video_height: u32,
  pub video_width: u32,
  pub width: f64,
  pub x: f64,
  pub y: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(
  rename_all = "camelCase",
  rename_all_fields = "camelCase",
  tag = "type"
)]
pub enum CursorRecord {
  Header {
    coordinate_space: String,
    platform: String,
    source: CursorSource,
    timebase: String,
    version: u16,
  },
  Appearance {
    height: f64,
    hotspot_x: f64,
    hotspot_y: f64,
    style: CursorStyle,
    timestamp_us: u64,
    width: f64,
  },
  Position {
    timestamp_us: u64,
    x: f64,
    y: f64,
  },
  Button {
    button: CursorButton,
    click_count: u8,
    state: ButtonState,
    timestamp_us: u64,
    x: f64,
    y: f64,
  },
}

/// Reads every complete record in a sidecar. A truncated final line from a
/// crashed process is ignored; every complete line before it remains useful.
pub fn read(path: &Path) -> Result<Vec<CursorRecord>, String> {
  let reader = BufReader::new(File::open(path).map_err(|error| error.to_string())?);
  let mut records = Vec::new();
  let mut parse_failure = None;
  for line in reader.lines() {
    let line = line.map_err(|error| error.to_string())?;
    if line.trim().is_empty() {
      continue;
    }
    match serde_json::from_str(&line) {
      Ok(record) => records.push(record),
      Err(error) => {
        parse_failure = Some(error.to_string());
        break;
      }
    }
  }
  match records.first() {
    Some(CursorRecord::Header { version, .. }) if *version == FORMAT_VERSION => Ok(records),
    Some(CursorRecord::Header { version, .. }) => Err(format!(
      "Cursor recording version {version} is not supported"
    )),
    _ => Err(parse_failure.map_or_else(
      || "The cursor recording has no valid header".to_owned(),
      |error| format!("The cursor recording has no valid header: {error}"),
    )),
  }
}
