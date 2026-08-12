// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { generatePalette, PaletteMode } from "../../lib/palette-generator";

export type MeshGradientPoint = {
  radiusX: number;
  radiusY: number;
  rotation: number;
  x: number;
  y: number;
};

const paletteModes: PaletteMode[] = ["bright", "chaotic", "dull", "shades"];
const MAXIMUM_MESH_COLORS = 5;

const random = (minimum: number, maximum: number) =>
  minimum + Math.random() * (maximum - minimum);

export const randomMeshComposition = (colorCount?: number) => {
  const pointCount = colorCount
    ? Math.max(3, Math.min(MAXIMUM_MESH_COLORS - 1, colorCount - 1))
    : 3 + Math.floor(Math.random() * 2);
  const mode = paletteModes[Math.floor(Math.random() * paletteModes.length)];
  const meshPoints = Array.from({ length: pointCount }, () => ({
    radiusX: random(38, 105),
    radiusY: random(28, 92),
    rotation: random(-180, 180),
    x: random(-18, 118),
    y: random(-18, 118),
  }));
  return {
    meshColors: generatePalette(mode, pointCount + 1),
    meshPoints,
    meshSeed: Math.floor(random(1, 65_535)),
    meshWarpPercent: random(5, 14),
  };
};
