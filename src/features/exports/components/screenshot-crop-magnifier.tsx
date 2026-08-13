// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  AnimatedPixelMagnifier,
  PixelMagnifierDirection,
} from "../../../components/shared/canvas-tools/pixel-magnifier";
import { ScreenshotLayout } from "../screenshot-output";

import { ScreenshotLayoutEdge } from "./screenshot-layout-geometry";

export function ScreenshotCropMagnifier({
  edges,
  inverseScale,
  layout,
  point,
  source,
  sourceImage,
}: {
  edges: ScreenshotLayoutEdge[];
  inverseScale: string;
  layout: ScreenshotLayout;
  point: { x: number; y: number };
  source: { height: number; width: number };
  sourceImage: CanvasImageSource;
}) {
  const direction = edges
    .map((edge, index) =>
      index === 0 ? edge : edge[0].toUpperCase() + edge.slice(1),
    )
    .join("") as PixelMagnifierDirection;
  return (
    <AnimatedPixelMagnifier
      className="pointer-events-none absolute z-10"
      direction={direction}
      point={{
        x: ((point.x - layout.image.x) / layout.image.width) * source.width,
        y: ((point.y - layout.image.y) / layout.image.height) * source.height,
      }}
      source={sourceImage}
      style={{
        height: `calc(96px * ${inverseScale})`,
        left: `${String(((point.x - layout.crop.x) / layout.crop.width) * 100)}%`,
        top: `${String(((point.y - layout.crop.y) / layout.crop.height) * 100)}%`,
        width: `calc(96px * ${inverseScale})`,
      }}
    />
  );
}
