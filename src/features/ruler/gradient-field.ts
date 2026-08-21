// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

export type Axis = "x" | "y";

/**
 * Per-pixel neighbour deltas produced by the Rust detector, plus the two
 * projection profiles derived from them once on arrival.
 *
 * `gx[y * width + x]` is the largest per-channel delta between pixel `x` and
 * pixel `x - 1` on the same row (column 0 is always 0); `gy` is the same
 * against the row above. An element covering columns `L..=R` therefore peaks at
 * `x == L` and at `x == R + 1`, so spans read off this field are half-open and
 * their width matches the element's true width.
 */
export type GradientField = {
  colSum: Float32Array;
  gx: Uint8Array;
  gy: Uint8Array;
  height: number;
  rowSum: Float32Array;
  width: number;
};

export type Peak = { position: number; score: number };
export type Profile = { from: number; values: Float32Array };

const PROFILE_SAMPLES = 64;

export const axisLength = (field: GradientField, axis: Axis) =>
  axis === "x" ? field.width : field.height;

export const crossAxis = (axis: Axis): Axis => (axis === "x" ? "y" : "x");

/** Gradient magnitude of the axis-relevant plane, 0 outside the field. */
export const gradientAt = (
  field: GradientField,
  { across, axis, position }: { across: number; axis: Axis; position: number },
) => {
  const x = axis === "x" ? position : across;
  const y = axis === "x" ? across : position;
  if (x < 0 || y < 0 || x >= field.width || y >= field.height) return 0;
  return (axis === "x" ? field.gx : field.gy)[y * field.width + x];
};

/** Mean gradient at `position`, averaged over the perpendicular range. */
const acrossMean = (
  field: GradientField,
  {
    axis,
    position,
    rangeEnd,
    rangeStart,
  }: { axis: Axis; position: number; rangeEnd: number; rangeStart: number },
) => {
  const limit = axisLength(field, crossAxis(axis)) - 1;
  const low = Math.round(Math.min(rangeStart, rangeEnd));
  const high = Math.round(Math.max(rangeStart, rangeEnd));
  const start = Math.max(0, Math.min(limit, low));
  const end = Math.max(start, Math.min(limit, high));
  const span = end - start;
  const samples = Math.max(1, Math.min(PROFILE_SAMPLES, span + 1));
  let total = 0;
  for (let sample = 0; sample < samples; sample += 1) {
    const across =
      samples === 1
        ? start
        : start + Math.round((sample / (samples - 1)) * span);
    total += gradientAt(field, { across, axis, position });
  }
  return total / samples;
};

/**
 * 1-D profile over `[from, to]` along `axis`, smoothed with [1, 2, 1] / 2.
 *
 * The divisor is deliberately 2, not the usual 4: it makes the kernel
 * mass-preserving for an isolated 1 px ridge, so a hard contrast-7 edge scores
 * 7 rather than 3.5, and an anti-aliased edge whose contrast is split across
 * two pixels (3, 4) still gathers to ~5.5. Dividing by 4 would halve every
 * isolated ridge and put subtle edges permanently out of reach of the
 * threshold. Plateaus double in value, which does not matter: profiles are
 * ridge-like and non-maximum suppression only compares neighbours.
 */
export const buildProfile = (
  field: GradientField,
  {
    axis,
    from,
    rangeEnd,
    rangeStart,
    to,
  }: {
    axis: Axis;
    from: number;
    rangeEnd: number;
    rangeStart: number;
    to: number;
  },
): Profile => {
  const limit = axisLength(field, axis) - 1;
  const start = Math.max(0, Math.min(limit, Math.round(from)));
  const end = Math.max(start, Math.min(limit, Math.round(to)));
  const raw = new Float32Array(end - start + 1);
  for (let index = 0; index < raw.length; index += 1)
    raw[index] = acrossMean(field, {
      axis,
      position: start + index,
      rangeEnd,
      rangeStart,
    });
  const values = new Float32Array(raw.length);
  for (let index = 0; index < raw.length; index += 1) {
    const left = raw[Math.max(0, index - 1)];
    const right = raw[Math.min(raw.length - 1, index + 1)];
    values[index] = (left + 2 * raw[index] + right) / 2;
  }
  return { from: start, values };
};

/** Non-maximum-suppressed local maxima clearing `threshold`, in field units. */
export const profilePeaks = (
  profile: Profile,
  threshold: number,
): readonly Peak[] => {
  const { from, values } = profile;
  const peaks: Peak[] = [];
  const last = values.length - 1;
  for (let index = 0; index <= last; index += 1) {
    const score = values[index];
    if (score < threshold) continue;
    const left = values[Math.max(0, index - 1)];
    const right = values[Math.min(last, index + 1)];
    if (score >= left && (score > right || index === last))
      peaks.push({ position: from + index, score });
  }
  return peaks;
};

/** Peak closest to `target`; ties go to the stronger one. */
export const nearestPeak = (peaks: readonly Peak[], target: number) => {
  let best: Peak | undefined;
  let bestDistance = Number.POSITIVE_INFINITY;
  for (const peak of peaks) {
    const distance = Math.abs(peak.position - target);
    if (
      distance < bestDistance ||
      (distance === bestDistance &&
        best !== undefined &&
        peak.score > best.score)
    ) {
      best = peak;
      bestDistance = distance;
    }
  }
  return best;
};
