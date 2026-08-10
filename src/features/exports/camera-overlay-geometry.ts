// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { CameraOverlaySettings, RecordingPreviewPane } from "./types";

export type OverlayRect = {
  height: number;
  width: number;
  x: number;
  y: number;
};

export type CameraOverlayGeometry = {
  camera: OverlayRect;
  frame: OverlayRect;
  radius: number;
};

export const RADIUS_HANDLE_INSET = 10;
export const RADIUS_HANDLE_TRAVEL = 0.55;
export const SCALE_GIZMO_DIMENSION = 600;
const CAMERA_PLACEMENT_PADDING_PERCENT = 2;
export const CAMERA_FRAME_BASE_WIDTH_PERCENT = 25;
const CAMERA_FRAME_MINIMUM_SHORT_EDGE_PERCENT = 10;

export const clamp = (value: number, minimum: number, maximum: number) =>
  Math.min(maximum, Math.max(minimum, value));

export const minimumCameraFrameWidth = (
  screen: RecordingPreviewPane,
  frame: OverlayRect,
) => {
  const minimumShortEdge =
    (Math.min(screen.width, screen.height) *
      CAMERA_FRAME_MINIMUM_SHORT_EDGE_PERCENT) /
    100;
  const aspectRatio = frame.width / frame.height;
  return aspectRatio >= 1 ? minimumShortEdge * aspectRatio : minimumShortEdge;
};

const nearestValue = (value: number, candidates: number[]) =>
  candidates.reduce((nearest, candidate) =>
    Math.abs(candidate - value) < Math.abs(nearest - value)
      ? candidate
      : nearest,
  );

/**
 * Snaps a camera frame to a 3 x 3 placement grid. One proportional padding
 * distance is used on both axes so the visual inset remains even.
 */
export const snapCameraFramePosition = ({
  frame,
  paddingPercent = CAMERA_PLACEMENT_PADDING_PERCENT,
  position,
  screen,
}: {
  frame: OverlayRect;
  position: { x: number; y: number };
  screen: RecordingPreviewPane;
  paddingPercent?: number;
}) => {
  const maximumX = Math.max(0, screen.width - frame.width);
  const maximumY = Math.max(0, screen.height - frame.height);
  const padding =
    (Math.min(screen.width, screen.height) * paddingPercent) / 100;

  return {
    x: nearestValue(position.x, [
      clamp(padding, 0, maximumX),
      maximumX / 2,
      clamp(maximumX - padding, 0, maximumX),
    ]),
    y: nearestValue(position.y, [
      clamp(padding, 0, maximumY),
      maximumY / 2,
      clamp(maximumY - padding, 0, maximumY),
    ]),
  };
};

export const cameraOverlayGeometry = (
  screen: RecordingPreviewPane,
  camera: RecordingPreviewPane,
  settings: CameraOverlaySettings,
): CameraOverlayGeometry => {
  const frame = {
    height: (screen.height * settings.frameHeightPercent) / 100,
    width: (screen.width * settings.frameWidthPercent) / 100,
    x: (screen.width * settings.frameXPercent) / 100,
    y: (screen.height * settings.frameYPercent) / 100,
  };
  const cameraWidth = (screen.width * settings.cameraWidthPercent) / 100;
  // The panes are independently rounded to whole preview pixels. Derive the
  // camera's height from the original media aspects so a crop that is valid
  // here reconstructs identically against the source files during export.
  const cameraHeightPercent =
    settings.cameraWidthPercent *
    (screen.sourceWidth / Math.max(1, screen.sourceHeight)) *
    (camera.sourceHeight / Math.max(1, camera.sourceWidth));
  const cameraHeight = (screen.height * cameraHeightPercent) / 100;
  const cameraCenterX = (screen.width * settings.cameraXPercent) / 100;
  const cameraCenterY = (screen.height * settings.cameraYPercent) / 100;
  return {
    camera: {
      height: cameraHeight,
      width: cameraWidth,
      x: cameraCenterX - cameraWidth / 2,
      y: cameraCenterY - cameraHeight / 2,
    },
    frame,
    radius:
      (Math.min(frame.width, frame.height) * settings.radiusPercent) / 100,
  };
};

/** Keyframeless' compact nonlinear scale-gizmo curve. */
export const scaleRingExtent = (percent: number, minimumDimension: number) => {
  const start = minimumDimension * 0.12;
  const span = minimumDimension * 0.057;
  if (percent <= 100) return start + (span * percent) / 100;
  return start + span + 2 * span * (Math.sqrt(1 + (percent - 100) / 100) - 1);
};

export const scalePercentFromRingExtent = (
  extent: number,
  minimumDimension: number,
) => {
  const start = minimumDimension * 0.12;
  const span = minimumDimension * 0.057;
  if (extent <= start + span)
    return clamp(((extent - start) * 100) / span, 1, 100);
  const normalized = (extent - start - span) / (2 * span) + 1;
  return clamp(100 + (normalized * normalized - 1) * 100, 100, 800);
};
