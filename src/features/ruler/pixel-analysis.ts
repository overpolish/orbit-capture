// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { axisSpanAt } from "./edge-detection";
import { Axis, GradientField } from "./gradient-field";

export type Point = { x: number; y: number };
export type Bounds = Point & { height: number; width: number };

export type PixelSnapshot = {
  height: number;
  pixels: Uint8ClampedArray;
  width: number;
};

export type PixelSize = { height: number; width: number };

export const orderedBounds = (start: Point, end: Point): Bounds => ({
  height: Math.abs(end.y - start.y),
  width: Math.abs(end.x - start.x),
  x: Math.min(start.x, end.x),
  y: Math.min(start.y, end.y),
});

export const screenToPixel = (
  point: Point,
  size: PixelSize,
  viewport: PixelSize,
): Point => ({
  x: Math.max(
    0,
    Math.min(
      size.width - 1,
      Math.floor((point.x / viewport.width) * size.width),
    ),
  ),
  y: Math.max(
    0,
    Math.min(
      size.height - 1,
      Math.floor((point.y / viewport.height) * size.height),
    ),
  ),
});

export const colorAt = (snapshot: PixelSnapshot, point: Point) => {
  const index =
    (Math.floor(point.y) * snapshot.width + Math.floor(point.x)) * 4;
  const red = snapshot.pixels[index] ?? 0;
  const green = snapshot.pixels[index + 1] ?? 0;
  const blue = snapshot.pixels[index + 2] ?? 0;
  return {
    blue,
    green,
    hex: `#${[red, green, blue]
      .map((channel) => channel.toString(16).padStart(2, "0"))
      .join("")}`.toUpperCase(),
    red,
  };
};

export type DistanceProbe = {
  axis: Axis;
  end: number;
  position: number;
  start: number;
};

export const distanceProbeAt = ({
  axis,
  field,
  point,
  threshold,
  viewport,
}: {
  axis: Axis;
  field: GradientField;
  point: Point;
  threshold: number;
  viewport: PixelSize;
}): DistanceProbe => {
  const span = axisSpanAt({
    axis,
    field,
    point: screenToPixel(point, field, viewport),
    threshold,
  });
  const scaleX = viewport.width / field.width;
  const scaleY = viewport.height / field.height;
  return axis === "x"
    ? {
        axis,
        end: span.end * scaleX,
        position: span.across * scaleY,
        start: span.start * scaleX,
      }
    : {
        axis,
        end: span.end * scaleY,
        position: span.across * scaleX,
        start: span.start * scaleY,
      };
};
