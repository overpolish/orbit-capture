// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  MeshGradientPoint,
  randomMeshComposition,
} from "./screenshot-background";

type ScreenshotBackgroundType = "mesh" | "solid";

export type ScreenshotOutputSettings = {
  backgroundColor: string;
  backgroundRadiusPercent: number;
  backgroundType: ScreenshotBackgroundType;
  dropShadow: boolean;
  height: number;
  meshColors: string[];
  meshLockedColors: boolean[];
  meshPoints: MeshGradientPoint[];
  meshSeed: number;
  meshWarpPercent: number;
  radiusPercent: number;
  screenshotCropHeightPercent: number;
  screenshotCropWidthPercent: number;
  screenshotCropXPercent: number;
  screenshotCropYPercent: number;
  screenshotImageWidthPercent: number;
  screenshotImageXPercent: number;
  screenshotImageYPercent: number;
  width: number;
};

export type RecordingOutputSettings = Record<
  "camera" | "primary",
  ScreenshotOutputSettings
>;

export const defaultScreenshotOutput = (
  width: number,
  height: number,
  radii: { background?: number; screenshot?: number } = {},
): ScreenshotOutputSettings => {
  const mesh = randomMeshComposition();
  return {
    ...mesh,
    backgroundColor: "#171717",
    backgroundRadiusPercent: radii.background ?? 0,
    backgroundType: "solid",
    dropShadow: true,
    height,
    meshLockedColors: mesh.meshColors.map(() => false),
    radiusPercent: radii.screenshot ?? 0,
    screenshotCropHeightPercent: 100,
    screenshotCropWidthPercent: 100,
    screenshotCropXPercent: 0,
    screenshotCropYPercent: 0,
    screenshotImageWidthPercent: 100,
    screenshotImageXPercent: 50,
    screenshotImageYPercent: 50,
    width,
  };
};

export const normalizedScreenshotOutput = (
  settings: ScreenshotOutputSettings,
): ScreenshotOutputSettings => {
  const finite = (value: number, fallback: number) =>
    Number.isFinite(value) ? value : fallback;
  const defaults = defaultScreenshotOutput(
    finite(settings.width, 1),
    finite(settings.height, 1),
  );
  return {
    ...settings,
    backgroundRadiusPercent: finite(
      settings.backgroundRadiusPercent,
      defaults.backgroundRadiusPercent,
    ),
    height: Math.max(1, Math.round(finite(settings.height, defaults.height))),
    meshLockedColors: settings.meshColors.map(
      (_, index) => settings.meshLockedColors[index] ?? false,
    ),
    meshPoints: settings.meshPoints.map((point, index) => {
      const fallback = defaults.meshPoints[index] ?? defaults.meshPoints[0];
      return {
        radiusX: finite(point.radiusX, fallback.radiusX),
        radiusY: finite(point.radiusY, fallback.radiusY),
        rotation: finite(point.rotation, fallback.rotation),
        x: finite(point.x, fallback.x),
        y: finite(point.y, fallback.y),
      };
    }),
    meshWarpPercent: finite(settings.meshWarpPercent, defaults.meshWarpPercent),
    radiusPercent: finite(settings.radiusPercent, defaults.radiusPercent),
    screenshotCropHeightPercent: finite(
      settings.screenshotCropHeightPercent,
      defaults.screenshotCropHeightPercent,
    ),
    screenshotCropWidthPercent: finite(
      settings.screenshotCropWidthPercent,
      defaults.screenshotCropWidthPercent,
    ),
    screenshotCropXPercent: finite(
      settings.screenshotCropXPercent,
      defaults.screenshotCropXPercent,
    ),
    screenshotCropYPercent: finite(
      settings.screenshotCropYPercent,
      defaults.screenshotCropYPercent,
    ),
    screenshotImageWidthPercent: finite(
      settings.screenshotImageWidthPercent,
      defaults.screenshotImageWidthPercent,
    ),
    screenshotImageXPercent: finite(
      settings.screenshotImageXPercent,
      defaults.screenshotImageXPercent,
    ),
    screenshotImageYPercent: finite(
      settings.screenshotImageYPercent,
      defaults.screenshotImageYPercent,
    ),
    width: Math.max(1, Math.round(finite(settings.width, defaults.width))),
  };
};

export const screenshotOutputDimensions = (
  settings: ScreenshotOutputSettings,
) => ({
  height: Math.max(1, Math.round(settings.height)),
  width: Math.max(1, Math.round(settings.width)),
});

const screenshotPlacement = (
  source: { height: number; width: number },
  output: { height: number; width: number },
) => {
  const scale = Math.min(
    output.width / source.width,
    output.height / source.height,
  );
  const width = Math.max(1, source.width * scale);
  const height = Math.max(1, source.height * scale);
  return {
    height,
    width,
    x: (output.width - width) / 2,
    y: (output.height - height) / 2,
  };
};

export type ScreenshotLayout = {
  crop: { height: number; width: number; x: number; y: number };
  image: { height: number; width: number; x: number; y: number };
};

export const screenshotLayout = (
  source: { height: number; width: number },
  output: { height: number; width: number },
  settings: ScreenshotOutputSettings,
): ScreenshotLayout => {
  const imageWidth =
    (output.width * Math.max(1, settings.screenshotImageWidthPercent)) / 100;
  const imageHeight = imageWidth * (source.height / Math.max(1, source.width));
  const imageCenterX = (output.width * settings.screenshotImageXPercent) / 100;
  const imageCenterY = (output.height * settings.screenshotImageYPercent) / 100;
  return {
    crop: {
      height: (output.height * settings.screenshotCropHeightPercent) / 100,
      width: (output.width * settings.screenshotCropWidthPercent) / 100,
      x: (output.width * settings.screenshotCropXPercent) / 100,
      y: (output.height * settings.screenshotCropYPercent) / 100,
    },
    image: {
      height: imageHeight,
      width: imageWidth,
      x: imageCenterX - imageWidth / 2,
      y: imageCenterY - imageHeight / 2,
    },
  };
};

export const resetScreenshotLayout = (
  settings: ScreenshotOutputSettings,
  source: { height: number; width: number },
): ScreenshotOutputSettings => {
  const output = screenshotOutputDimensions(settings);
  const placement = screenshotPlacement(source, output);
  return {
    ...settings,
    screenshotCropHeightPercent: (placement.height * 100) / output.height,
    screenshotCropWidthPercent: (placement.width * 100) / output.width,
    screenshotCropXPercent: (placement.x * 100) / output.width,
    screenshotCropYPercent: (placement.y * 100) / output.height,
    screenshotImageWidthPercent: (placement.width * 100) / output.width,
    screenshotImageXPercent:
      ((placement.x + placement.width / 2) * 100) / output.width,
    screenshotImageYPercent:
      ((placement.y + placement.height / 2) * 100) / output.height,
  };
};

export const defaultRecordingOutput = ({
  camera,
  primary,
}: {
  primary: { height: number; width: number };
  camera?: { height: number; width: number } | null;
}): RecordingOutputSettings => ({
  camera: resetScreenshotLayout(
    defaultScreenshotOutput(camera?.width ?? 1, camera?.height ?? 1),
    camera ?? { height: 1, width: 1 },
  ),
  primary: resetScreenshotLayout(
    defaultScreenshotOutput(primary.width, primary.height),
    primary,
  ),
});

export const restoredRecordingOutput = ({
  camera,
  persisted,
  primary,
}: {
  primary: { height: number; width: number };
  camera?: { height: number; width: number } | null;
  persisted?: RecordingOutputSettings | null;
}): RecordingOutputSettings => {
  const defaults = defaultRecordingOutput({ camera, primary });
  if (!persisted) return defaults;
  const restore = (
    key: keyof RecordingOutputSettings,
    source: { height: number; width: number },
  ) =>
    resetScreenshotLayout(
      {
        ...defaults[key],
        ...persisted[key],
        backgroundRadiusPercent: 0,
        height: defaults[key].height,
        width: defaults[key].width,
      },
      source,
    );
  return {
    camera: restore("camera", camera ?? { height: 1, width: 1 }),
    primary: restore("primary", primary),
  };
};

export const hasOutputComposition = (
  settings: ScreenshotOutputSettings,
  source: { height: number; width: number },
) =>
  settings.width !== source.width ||
  settings.height !== source.height ||
  settings.backgroundRadiusPercent > 0 ||
  settings.radiusPercent > 0 ||
  Math.abs(settings.screenshotCropHeightPercent - 100) > 0.000_001 ||
  Math.abs(settings.screenshotCropWidthPercent - 100) > 0.000_001 ||
  Math.abs(settings.screenshotCropXPercent) > 0.000_001 ||
  Math.abs(settings.screenshotCropYPercent) > 0.000_001 ||
  Math.abs(settings.screenshotImageWidthPercent - 100) > 0.000_001 ||
  Math.abs(settings.screenshotImageXPercent - 50) > 0.000_001 ||
  Math.abs(settings.screenshotImageYPercent - 50) > 0.000_001;
