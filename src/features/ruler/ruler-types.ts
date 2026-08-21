// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Bounds } from "./pixel-analysis";

/** `from` is the pre-snap drag rect, kept only to seed the settle animation. */
export type Measurement = Bounds & { id: number; from?: Bounds };
/**
 * `anchor` is the cross-axis world coordinate the guide was placed at; gap
 * chips sit at the midpoint of their pair's anchors instead of chasing the
 * cursor.
 */
export type Guide = {
  anchor: number;
  axis: "x" | "y";
  id: number;
  position: number;
  transient?: boolean;
};

export type DistanceProbe = {
  axis: "x" | "y";
  end: number;
  position: number;
  start: number;
  id?: number;
};
