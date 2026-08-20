// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

/// Frame resize edge mask shared by the native input adapters.
///
/// The values intentionally match the existing Metal/D3D gesture protocol:
/// left=1, right=2, top=4, bottom=8 and centered=1<<16.
pub const FRAME_EDGE_LEFT: u32 = 1;
pub const FRAME_EDGE_RIGHT: u32 = 1 << 1;
pub const FRAME_EDGE_TOP: u32 = 1 << 2;
pub const FRAME_EDGE_BOTTOM: u32 = 1 << 3;
pub const FRAME_EDGE_CENTERED: u32 = 1 << 16;
pub(super) const FRAME_MIN_SIZE: f64 = 64.0;
pub(super) const FRAME_MAX_AREA: f64 = 120_000_000.0;
