// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

export type RulerTolerance = "low" | "medium" | "high";

// Edge-mass cutoffs, in u8 contrast units summed across an edge's ramp (see
// `buildProfile` and `detect_boxes`). Low tolerance keeps only strong edges;
// high tolerance also catches the subtle ones, at the cost of picking up text
// speckle. `high` is 5 so that a #F8F9FA-on-white card is reachable: a hard
// contrast-7 edge scores 7, and the same edge anti-aliased into a 3/4 split
// still gathers to ~5.5, while a 2-per-pixel background ramp stays out at 4.
const RULER_TOLERANCES = [
  { id: "low", threshold: 48 },
  { id: "medium", threshold: 24 },
  { id: "high", threshold: 5 },
] as const;

export const toleranceThreshold = (tolerance: RulerTolerance) =>
  RULER_TOLERANCES.find(({ id }) => id === tolerance)?.threshold ?? 24;

export const nextTolerance = (tolerance: RulerTolerance): RulerTolerance =>
  tolerance === "low" ? "medium" : tolerance === "medium" ? "high" : "low";
