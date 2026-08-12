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
): ScreenshotOutputSettings => ({
  ...settings,
  meshLockedColors: settings.meshColors.map(
    (_, index) => settings.meshLockedColors[index] ?? false,
  ),
});

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
