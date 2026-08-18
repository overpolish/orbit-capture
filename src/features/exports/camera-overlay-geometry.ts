// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  screenshotLayout,
  screenshotOutputDimensions,
  ScreenshotOutputSettings,
} from "./screenshot-output";
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

/** Display-only overlay used by the native compositor while camera crop is active. */
export const uncroppedCameraPreviewOverlay = (
  screen: RecordingPreviewPane,
  camera: RecordingPreviewPane,
  settings: CameraOverlaySettings,
): CameraOverlaySettings => {
  const { camera: image } = cameraOverlayGeometry(screen, camera, settings);
  return {
    ...settings,
    frameHeightPercent: (image.height * 100) / screen.height,
    frameWidthPercent: (image.width * 100) / screen.width,
    frameXPercent: (image.x * 100) / screen.width,
    frameYPercent: (image.y * 100) / screen.height,
    radiusPercent: 0,
  };
};

/**
 * Carry a camera crop from split-track output into the baked overlay frame.
 *
 * The split preview stores the camera crop in the camera track's output
 * settings, while baked preview stores the same visible window as the
 * overlay's frame rectangle. Converting at the bake boundary keeps the crop
 * visually and semantically intact instead of resetting it to the full camera
 * image.
 */
export const cameraOverlayWithCameraCrop = ({
  cameraOutput,
  cameraSource,
  screenOutput,
  settings,
}: {
  cameraOutput: ScreenshotOutputSettings;
  cameraSource: { height: number; width: number };
  screenOutput: { height: number; width: number };
  settings: CameraOverlaySettings;
}): CameraOverlaySettings => {
  const screen = {
    height: screenOutput.height,
    kind: "screen" as const,
    sourceHeight: screenOutput.height,
    sourceWidth: screenOutput.width,
    width: screenOutput.width,
    x: 0,
    y: 0,
  };
  const camera = {
    height: cameraSource.height,
    kind: "camera" as const,
    sourceHeight: cameraSource.height,
    sourceWidth: cameraSource.width,
    width: cameraSource.width,
    x: 0,
    y: 0,
  };
  const geometry = cameraOverlayGeometry(screen, camera, settings);
  const layout = screenshotLayout(
    cameraSource,
    screenshotOutputDimensions(cameraOutput),
    cameraOutput,
  );
  const cropLeft =
    geometry.camera.x +
    ((layout.crop.x - layout.image.x) / Math.max(1, layout.image.width)) *
      geometry.camera.width;
  const cropTop =
    geometry.camera.y +
    ((layout.crop.y - layout.image.y) / Math.max(1, layout.image.height)) *
      geometry.camera.height;
  const cropWidth =
    (layout.crop.width / Math.max(1, layout.image.width)) *
    geometry.camera.width;
  const cropHeight =
    (layout.crop.height / Math.max(1, layout.image.height)) *
    geometry.camera.height;

  return {
    ...settings,
    frameHeightPercent: (cropHeight * 100) / Math.max(1, screen.height),
    frameWidthPercent: (cropWidth * 100) / Math.max(1, screen.width),
    frameXPercent: (cropLeft * 100) / Math.max(1, screen.width),
    frameYPercent: (cropTop * 100) / Math.max(1, screen.height),
  };
};

/** Convert the baked overlay frame back into the split camera crop settings. */
export const cameraOutputWithOverlayCrop = ({
  cameraOutput,
  cameraSource,
  screenOutput,
  settings,
}: {
  cameraOutput: ScreenshotOutputSettings;
  cameraSource: { height: number; width: number };
  screenOutput: { height: number; width: number };
  settings: CameraOverlaySettings;
}): ScreenshotOutputSettings => {
  const screen = {
    height: screenOutput.height,
    kind: "screen" as const,
    sourceHeight: screenOutput.height,
    sourceWidth: screenOutput.width,
    width: screenOutput.width,
    x: 0,
    y: 0,
  };
  const camera = {
    height: cameraSource.height,
    kind: "camera" as const,
    sourceHeight: cameraSource.height,
    sourceWidth: cameraSource.width,
    width: cameraSource.width,
    x: 0,
    y: 0,
  };
  const geometry = cameraOverlayGeometry(screen, camera, settings);
  const layout = screenshotLayout(
    cameraSource,
    screenshotOutputDimensions(cameraOutput),
    cameraOutput,
  );
  const cropX =
    layout.image.x +
    ((geometry.frame.x - geometry.camera.x) /
      Math.max(1, geometry.camera.width)) *
      layout.image.width;
  const cropY =
    layout.image.y +
    ((geometry.frame.y - geometry.camera.y) /
      Math.max(1, geometry.camera.height)) *
      layout.image.height;
  const cropWidth =
    (geometry.frame.width / Math.max(1, geometry.camera.width)) *
    layout.image.width;
  const cropHeight =
    (geometry.frame.height / Math.max(1, geometry.camera.height)) *
    layout.image.height;
  const output = screenshotOutputDimensions(cameraOutput);
  return {
    ...cameraOutput,
    screenshotCropHeightPercent:
      (cropHeight * 100) / Math.max(1, output.height),
    screenshotCropWidthPercent: (cropWidth * 100) / Math.max(1, output.width),
    screenshotCropXPercent: (cropX * 100) / Math.max(1, output.width),
    screenshotCropYPercent: (cropY * 100) / Math.max(1, output.height),
  };
};

/** Whether the overlay frame is acting as a crop window rather than the full image. */
export const cameraOverlayHasCrop = ({
  cameraSource,
  screenOutput,
  settings,
}: {
  cameraSource: { height: number; width: number };
  screenOutput: { height: number; width: number };
  settings: CameraOverlaySettings;
}) => {
  const screen = {
    height: screenOutput.height,
    kind: "screen" as const,
    sourceHeight: screenOutput.height,
    sourceWidth: screenOutput.width,
    width: screenOutput.width,
    x: 0,
    y: 0,
  };
  const camera = {
    height: cameraSource.height,
    kind: "camera" as const,
    sourceHeight: cameraSource.height,
    sourceWidth: cameraSource.width,
    width: cameraSource.width,
    x: 0,
    y: 0,
  };
  const geometry = cameraOverlayGeometry(screen, camera, settings);
  const epsilon = 0.000_001;
  return (
    Math.abs(geometry.frame.x - geometry.camera.x) > epsilon ||
    Math.abs(geometry.frame.y - geometry.camera.y) > epsilon ||
    Math.abs(geometry.frame.width - geometry.camera.width) > epsilon ||
    Math.abs(geometry.frame.height - geometry.camera.height) > epsilon
  );
};

/** Grow the output canvas around a camera frame dragged past its edges. */
export const fitCanvasToCameraOverlay = (
  screen: RecordingPreviewPane,
  camera: RecordingPreviewPane,
  settings: CameraOverlaySettings,
) => {
  const { frame } = cameraOverlayGeometry(screen, camera, settings);
  const left = Math.min(0, Math.floor(frame.x));
  const top = Math.min(0, Math.floor(frame.y));
  const right = Math.max(screen.width, Math.ceil(frame.x + frame.width));
  const bottom = Math.max(screen.height, Math.ceil(frame.y + frame.height));
  return {
    height: bottom - top,
    originX: left,
    originY: top,
    width: right - left,
  };
};

/** Preserve baked-camera geometry while the shared output canvas is resized. */
export const resizeCameraOverlayCanvas = (
  settings: CameraOverlaySettings,
  previous: { height: number; width: number },
  bounds: { height: number; originX: number; originY: number; width: number },
): CameraOverlaySettings => {
  const width = Math.max(1, bounds.width);
  const height = Math.max(1, bounds.height);
  const frameX = (previous.width * settings.frameXPercent) / 100;
  const frameY = (previous.height * settings.frameYPercent) / 100;
  const cameraX = (previous.width * settings.cameraXPercent) / 100;
  const cameraY = (previous.height * settings.cameraYPercent) / 100;
  return {
    ...settings,
    cameraWidthPercent: (previous.width * settings.cameraWidthPercent) / width,
    cameraXPercent: ((cameraX - bounds.originX) * 100) / width,
    cameraYPercent: ((cameraY - bounds.originY) * 100) / height,
    frameHeightPercent:
      (previous.height * settings.frameHeightPercent) / height,
    frameWidthPercent: (previous.width * settings.frameWidthPercent) / width,
    frameXPercent: ((frameX - bounds.originX) * 100) / width,
    frameYPercent: ((frameY - bounds.originY) * 100) / height,
  };
};

/** Uniformly reframe a baked camera with the shared output dimensions. */
export const resizeCameraOverlayCentered = (
  settings: CameraOverlaySettings,
  previous: { height: number; width: number },
  next: { height: number; width: number },
): CameraOverlaySettings => {
  const width = Math.max(1, next.width);
  const height = Math.max(1, next.height);
  const scale = Math.min(width / previous.width, height / previous.height);
  const offsetX = (width - previous.width * scale) / 2;
  const offsetY = (height - previous.height * scale) / 2;
  const transformX = (value: number) => offsetX + value * scale;
  const transformY = (value: number) => offsetY + value * scale;
  return {
    ...settings,
    cameraWidthPercent:
      (previous.width * settings.cameraWidthPercent * scale) / width,
    cameraXPercent:
      (transformX((previous.width * settings.cameraXPercent) / 100) * 100) /
      width,
    cameraYPercent:
      (transformY((previous.height * settings.cameraYPercent) / 100) * 100) /
      height,
    frameHeightPercent:
      (previous.height * settings.frameHeightPercent * scale) / height,
    frameWidthPercent:
      (previous.width * settings.frameWidthPercent * scale) / width,
    frameXPercent:
      (transformX((previous.width * settings.frameXPercent) / 100) * 100) /
      width,
    frameYPercent:
      (transformY((previous.height * settings.frameYPercent) / 100) * 100) /
      height,
  };
};
