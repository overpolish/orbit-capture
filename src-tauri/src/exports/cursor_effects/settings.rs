// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorEffectSettings {
  pub bake: bool,
  pub click_animation: bool,
  pub motion_blur: bool,
  pub smooth_movement: bool,
  pub size_percent: f64,
}

impl Default for CursorEffectSettings {
  fn default() -> Self {
    Self {
      bake: true,
      click_animation: true,
      motion_blur: true,
      smooth_movement: true,
      size_percent: 100.0,
    }
  }
}
