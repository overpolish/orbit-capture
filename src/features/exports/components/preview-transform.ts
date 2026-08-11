// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

/** Zoom is relative to fit, so 1 is "the whole capture on screen". */
export const FIT = 1;
/** The capture may be reduced below fit to inspect it with space around it. */
export const MINIMUM_ZOOM = 0.1;
/** Small captures can still be magnified even when that passes native pixels. */
const MIN_MAX_ZOOM = 4;

export type PreviewTransform = { x: number; y: number; zoom: number };
export type PreviewGeometry = {
  boxHeight: number;
  boxWidth: number;
  fitScale: number;
  naturalHeight: number;
  naturalWidth: number;
};

export const clamp = (value: number, min: number, max: number) =>
  Math.min(Math.max(value, min), max);

/** Zooming all the way in lands on the capture's own pixels, exactly 1:1. */
export const maximumZoom = (geometry: PreviewGeometry) =>
  Math.max(MIN_MAX_ZOOM, 1 / (geometry.fitScale || 1));
