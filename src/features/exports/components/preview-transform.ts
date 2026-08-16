// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

/** Zoom is relative to fit, so 1 is "the whole capture on screen". */
export const FIT = 1;
/** The capture may be reduced below fit to inspect it with space around it. */
export const MINIMUM_ZOOM = 0.1;
/** Small captures can still be magnified even when that passes native pixels. */
const MIN_MAX_ZOOM = 4;
/** Keep every zoom input on the same finite range as the toolbar. */
export const MAXIMUM_ZOOM = 16;
/** Core Animation and DirectComposition textures share this practical cap. */
const MAXIMUM_NATIVE_SURFACE_EDGE = 16_384;

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
export const maximumZoom = (geometry: PreviewGeometry) => {
  const fitScale = geometry.fitScale || 1;
  const pixelRatio = globalThis.devicePixelRatio || 1;
  const nativeSurfaceLimit = Math.min(
    MAXIMUM_NATIVE_SURFACE_EDGE /
      Math.max(1, geometry.naturalWidth * fitScale * pixelRatio),
    MAXIMUM_NATIVE_SURFACE_EDGE /
      Math.max(1, geometry.naturalHeight * fitScale * pixelRatio),
  );
  return Math.max(
    MINIMUM_ZOOM,
    Math.min(
      MAXIMUM_ZOOM,
      Math.max(MIN_MAX_ZOOM, 1 / fitScale),
      nativeSurfaceLimit,
    ),
  );
};
