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

type ScreenshotWorkspaceItemOutput = {
  id: number;
  output: ScreenshotOutputSettings;
};

export type ScreenshotWorkspaceOutputSettings = ScreenshotOutputSettings & {
  items: ScreenshotWorkspaceItemOutput[];
};

export const normalizedScreenshotWorkspaceOutput = (
  settings: ScreenshotWorkspaceOutputSettings,
): ScreenshotWorkspaceOutputSettings => ({
  ...normalizedScreenshotOutput(settings),
  items: settings.items.map((item) => ({
    id: item.id,
    output: normalizedScreenshotOutput(item.output),
  })),
});

export const screenshotWorkspaceItemOutput = (
  settings: ScreenshotWorkspaceOutputSettings,
  id: number,
) => {
  const output = settings.items.find((item) => item.id === id)?.output;
  if (!output) return settings;
  return {
    ...output,
    backgroundColor: settings.backgroundColor,
    backgroundRadiusPercent: settings.backgroundRadiusPercent,
    backgroundType: settings.backgroundType,
    height: settings.height,
    meshColors: settings.meshColors,
    meshLockedColors: settings.meshLockedColors,
    meshPoints: settings.meshPoints,
    meshSeed: settings.meshSeed,
    meshWarpPercent: settings.meshWarpPercent,
    width: settings.width,
  };
};

export type RecordingOutputSettings = Record<
  "camera" | "primary",
  ScreenshotOutputSettings
> & {
  /** Front-to-back order for the two video layers. */
  cameraOnTop: boolean;
};

export const recordingVideoTrackOrder = (settings: RecordingOutputSettings) =>
  settings.cameraOnTop
    ? (["camera", "primary"] as const)
    : (["primary", "camera"] as const);

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

export type ScreenshotCanvasBounds = {
  height: number;
  originX: number;
  originY: number;
  width: number;
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

/** Resize the output canvas without moving or scaling the screenshot in it. */
export const resizeScreenshotCanvas = (
  source: { height: number; width: number },
  settings: ScreenshotOutputSettings,
  bounds: ScreenshotCanvasBounds,
): ScreenshotOutputSettings => {
  const previousOutput = screenshotOutputDimensions(settings);
  const layout = screenshotLayout(source, previousOutput, settings);
  const width = Math.max(1, Math.round(bounds.width));
  const height = Math.max(1, Math.round(bounds.height));
  const cropX = layout.crop.x - bounds.originX;
  const cropY = layout.crop.y - bounds.originY;
  const imageCenterX = layout.image.x + layout.image.width / 2 - bounds.originX;
  const imageCenterY =
    layout.image.y + layout.image.height / 2 - bounds.originY;
  return {
    ...settings,
    height,
    screenshotCropHeightPercent: (layout.crop.height * 100) / height,
    screenshotCropWidthPercent: (layout.crop.width * 100) / width,
    screenshotCropXPercent: (cropX * 100) / width,
    screenshotCropYPercent: (cropY * 100) / height,
    screenshotImageWidthPercent: (layout.image.width * 100) / width,
    screenshotImageXPercent: (imageCenterX * 100) / width,
    screenshotImageYPercent: (imageCenterY * 100) / height,
    width,
  };
};

/** Uniformly scale the composition into inspector-entered canvas dimensions. */
export const resizeScreenshotWorkspaceCentered = ({
  height,
  settings,
  sources,
  width,
}: {
  height: number;
  settings: ScreenshotWorkspaceOutputSettings;
  sources: { height: number; id: number; width: number }[];
  width: number;
}): ScreenshotWorkspaceOutputSettings => {
  const previous = screenshotOutputDimensions(settings);
  const nextWidth = Math.max(1, Math.round(width));
  const nextHeight = Math.max(1, Math.round(height));
  const scale = Math.min(
    nextWidth / previous.width,
    nextHeight / previous.height,
  );
  const nextCenterX = nextWidth / 2;
  const nextCenterY = nextHeight / 2;
  return {
    ...settings,
    height: nextHeight,
    items: settings.items.map((itemOutput) => {
      const source = sources.find(
        (candidate) => candidate.id === itemOutput.id,
      );
      if (!source) return itemOutput;
      const itemSettings = screenshotWorkspaceItemOutput(
        settings,
        itemOutput.id,
      );
      const layout = screenshotLayout(source, previous, itemSettings);
      const transformPoint = (x: number, y: number) => ({
        x: nextCenterX + (x - previous.width / 2) * scale,
        y: nextCenterY + (y - previous.height / 2) * scale,
      });
      const cropOrigin = transformPoint(layout.crop.x, layout.crop.y);
      const imageCenter = transformPoint(
        layout.image.x + layout.image.width / 2,
        layout.image.y + layout.image.height / 2,
      );
      return {
        ...itemOutput,
        output: {
          ...itemSettings,
          height: nextHeight,
          screenshotCropHeightPercent:
            (layout.crop.height * scale * 100) / nextHeight,
          screenshotCropWidthPercent:
            (layout.crop.width * scale * 100) / nextWidth,
          screenshotCropXPercent: (cropOrigin.x * 100) / nextWidth,
          screenshotCropYPercent: (cropOrigin.y * 100) / nextHeight,
          screenshotImageWidthPercent:
            (layout.image.width * scale * 100) / nextWidth,
          screenshotImageXPercent: (imageCenter.x * 100) / nextWidth,
          screenshotImageYPercent: (imageCenter.y * 100) / nextHeight,
          width: nextWidth,
        },
      };
    }),
    width: nextWidth,
  };
};

/** Uniformly resize a single recording output around its current centre. */
export const resizeScreenshotOutputCentered = ({
  height,
  settings,
  source,
  width,
}: {
  height: number;
  settings: ScreenshotOutputSettings;
  source: { height: number; width: number };
  width: number;
}): ScreenshotOutputSettings =>
  screenshotWorkspaceItemOutput(
    resizeScreenshotWorkspaceCentered({
      height,
      settings: {
        ...settings,
        items: [{ id: 0, output: settings }],
      },
      sources: [{ ...source, id: 0 }],
      width,
    }),
    0,
  );

/** Grow around visible items, using the gesture-start canvas as the floor. */
export const fitScreenshotWorkspaceToItems = ({
  initial,
  movedItemId,
  movedItemOutput,
  sources,
}: {
  initial: ScreenshotWorkspaceOutputSettings;
  movedItemId: number;
  movedItemOutput: ScreenshotOutputSettings;
  sources: { height: number; id: number; width: number }[];
}): {
  bounds: ScreenshotCanvasBounds;
  movedItemOutput: ScreenshotOutputSettings;
  output: ScreenshotWorkspaceOutputSettings;
} => {
  const initialSize = screenshotOutputDimensions(initial);
  const sourceById = new Map(sources.map((source) => [source.id, source]));
  const movedItems = initial.items.map((item) => ({
    ...item,
    output:
      item.id === movedItemId
        ? movedItemOutput
        : screenshotWorkspaceItemOutput(initial, item.id),
  }));
  let left = 0;
  let top = 0;
  let right = initialSize.width;
  let bottom = initialSize.height;
  for (const item of movedItems) {
    const source = sourceById.get(item.id);
    if (!source) continue;
    const crop = screenshotLayout(source, initialSize, item.output).crop;
    left = Math.min(left, Math.floor(crop.x));
    top = Math.min(top, Math.floor(crop.y));
    right = Math.max(right, Math.ceil(crop.x + crop.width));
    bottom = Math.max(bottom, Math.ceil(crop.y + crop.height));
  }
  const bounds = {
    height: bottom - top,
    originX: left,
    originY: top,
    width: right - left,
  };
  const items = movedItems.map((item) => {
    const source = sourceById.get(item.id);
    return source
      ? { ...item, output: resizeScreenshotCanvas(source, item.output, bounds) }
      : item;
  });
  const output = {
    ...initial,
    height: bounds.height,
    items,
    width: bounds.width,
  };
  return {
    bounds,
    movedItemOutput:
      items.find((item) => item.id === movedItemId)?.output ?? movedItemOutput,
    output,
  };
};

/**
 * Display-only composition used while the crop tool is active. The native
 * compositor draws the whole source image; the webview then dims the pixels
 * outside the committed crop without changing what will be exported.
 */
export const uncroppedScreenshotPreviewOutput = (
  source: { height: number; width: number },
  settings: ScreenshotOutputSettings,
): ScreenshotOutputSettings => {
  const output = screenshotOutputDimensions(settings);
  const { image } = screenshotLayout(source, output, settings);
  return {
    ...settings,
    dropShadow: false,
    radiusPercent: 0,
    screenshotCropHeightPercent: (image.height * 100) / output.height,
    screenshotCropWidthPercent: (image.width * 100) / output.width,
    screenshotCropXPercent: (image.x * 100) / output.width,
    screenshotCropYPercent: (image.y * 100) / output.height,
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

/** Reset only the visible crop, retaining the image's scale and position. */
export const resetScreenshotCrop = (
  settings: ScreenshotOutputSettings,
  source: { height: number; width: number },
): ScreenshotOutputSettings => {
  const output = screenshotOutputDimensions(settings);
  const { image } = screenshotLayout(source, output, settings);
  return {
    ...settings,
    radiusPercent: 0,
    screenshotCropHeightPercent: (image.height * 100) / output.height,
    screenshotCropWidthPercent: (image.width * 100) / output.width,
    screenshotCropXPercent: (image.x * 100) / output.width,
    screenshotCropYPercent: (image.y * 100) / output.height,
  };
};

/** Reset the selected item's scale and position while retaining its crop. */
export const resetScreenshotTransform = (
  settings: ScreenshotOutputSettings,
  source: { height: number; width: number },
): ScreenshotOutputSettings => {
  const output = screenshotOutputDimensions(settings);
  const current = screenshotLayout(source, output, settings);
  const target = screenshotPlacement(
    { height: current.crop.height, width: current.crop.width },
    output,
  );
  const scale = target.width / Math.max(1, current.crop.width);
  const imageX = target.x + (current.image.x - current.crop.x) * scale;
  const imageY = target.y + (current.image.y - current.crop.y) * scale;
  const imageWidth = current.image.width * scale;
  const imageHeight = current.image.height * scale;
  return {
    ...settings,
    screenshotCropHeightPercent: (target.height * 100) / output.height,
    screenshotCropWidthPercent: (target.width * 100) / output.width,
    screenshotCropXPercent: (target.x * 100) / output.width,
    screenshotCropYPercent: (target.y * 100) / output.height,
    screenshotImageWidthPercent: (imageWidth * 100) / output.width,
    screenshotImageXPercent: ((imageX + imageWidth / 2) * 100) / output.width,
    screenshotImageYPercent: ((imageY + imageHeight / 2) * 100) / output.height,
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
  cameraOnTop: true,
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
  persisted?: Partial<RecordingOutputSettings> | null;
}): RecordingOutputSettings => {
  const defaults = defaultRecordingOutput({ camera, primary });
  if (!persisted) return defaults;
  const restore = (
    key: "camera" | "primary",
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
    // Layer order belongs to the current recording, not the previous export.
    // A fresh recording always starts with its camera above the screen.
    cameraOnTop: true,
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
