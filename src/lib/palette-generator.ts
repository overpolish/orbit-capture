// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

export type PaletteMode = "bright" | "chaotic" | "dull" | "shades";

type Hsl = { h: number; l: number; s: number };
type Point = { x: number; y: number; z: number };
type Range = [number, number];
type PaletteConfiguration = {
  anchors: number;
  dark: Range;
  easing: (position: number) => number;
  gap: Range;
  light: Range;
  saturation: Range;
};

const random = (minimum: number, maximum: number) =>
  minimum + Math.random() * (maximum - minimum);
const clamp = (value: number, minimum: number, maximum: number) =>
  Math.min(maximum, Math.max(minimum, value));
const lerp = (start: number, end: number, position: number) =>
  start + (end - start) * position;
const smooth = (position: number) => position * position * (3 - 2 * position);
const sinusoidal = (position: number) => Math.sin((position * Math.PI) / 2);

const configurations = {
  bright: {
    anchors: 2,
    dark: [0.5, 0.62],
    easing: sinusoidal,
    gap: [40, 95],
    light: [0.7, 0.82],
    saturation: [0.6, 0.9],
  },
  chaotic: {
    anchors: 3,
    dark: [0.28, 0.42],
    easing: sinusoidal,
    gap: [55, 150],
    light: [0.8, 0.9],
    saturation: [0.55, 0.9],
  },
  dull: {
    anchors: 2,
    dark: [0.34, 0.46],
    easing: smooth,
    gap: [45, 105],
    light: [0.62, 0.74],
    saturation: [0.2, 0.4],
  },
  shades: {
    anchors: 2,
    dark: [0.12, 0.2],
    easing: smooth,
    gap: [0, 0],
    light: [0.86, 0.95],
    saturation: [0.5, 0.72],
  },
} satisfies Record<PaletteMode, PaletteConfiguration>;

const hslPoint = ({ h, l, s }: Hsl): Point => {
  const angle = (h * Math.PI) / 180;
  return {
    x: 0.5 + l * 0.5 * Math.cos(angle),
    y: 0.5 + l * 0.5 * Math.sin(angle),
    z: s,
  };
};

const pointHsl = ({ x, y, z }: Point): Hsl => ({
  h: ((Math.atan2(y - 0.5, x - 0.5) * 180) / Math.PI + 360) % 360,
  l: Math.hypot(x - 0.5, y - 0.5) * 2,
  s: z,
});

const hslHex = ({ h, l, s }: Hsl) => {
  const channel = (offset: number) => {
    const part = (offset + h / 30) % 12;
    const chroma = s * Math.min(l, 1 - l);
    return l - chroma * Math.max(-1, Math.min(part - 3, 9 - part, 1));
  };
  return `#${[channel(0), channel(8), channel(4)]
    .map((value) =>
      Math.round(clamp(value, 0, 1) * 255)
        .toString(16)
        .padStart(2, "0"),
    )
    .join("")}`.toUpperCase();
};

const hexHsl = (value: string): Hsl => {
  const hex = value.replace("#", "");
  const [r, g, b] = [0, 2, 4].map(
    (offset) => Number.parseInt(hex.slice(offset, offset + 2), 16) / 255,
  );
  const maximum = Math.max(r, g, b);
  const minimum = Math.min(r, g, b);
  const delta = maximum - minimum;
  const l = (maximum + minimum) / 2;
  const s = delta === 0 ? 0 : delta / (1 - Math.abs(2 * l - 1));
  let h = 0;
  if (delta !== 0) {
    if (maximum === r) h = 60 * (((g - b) / delta) % 6);
    else if (maximum === g) h = 60 * ((b - r) / delta + 2);
    else h = 60 * ((r - g) / delta + 4);
  }
  return { h: (h + 360) % 360, l, s };
};

export function generatePalette(mode: PaletteMode, count: number) {
  const configuration = configurations[mode];
  const sameHue = mode === "shades";
  const direction = Math.random() < 0.5 ? -1 : 1;
  const startHue = random(0, 360);
  const anchorCount = sameHue
    ? 2
    : Math.min(6, Math.max(configuration.anchors, Math.round(count / 2) + 1));
  const anchors: Point[] = [];
  let hue = startHue;
  for (let index = 0; index < anchorCount; index += 1) {
    if (index > 0 && !sameHue) hue += random(...configuration.gap) * direction;
    const lightness =
      index % 2 === 0 && !sameHue
        ? random(...configuration.light)
        : index === anchorCount - 1 && sameHue
          ? random(...configuration.light)
          : random(...configuration.dark);
    anchors.push(
      hslPoint({
        h: sameHue ? startHue : hue,
        l: lightness,
        s: random(...configuration.saturation),
      }),
    );
  }
  return Array.from({ length: count }, (_, index) => {
    const position = count > 1 ? index / (count - 1) : 0.5;
    const scaled = position * (anchors.length - 1);
    const segment = Math.min(anchors.length - 2, Math.floor(scaled));
    const local = configuration.easing(scaled - segment);
    const start = anchors[segment];
    const end = anchors[segment + 1];
    return hslHex(
      pointHsl({
        x: lerp(start.x, end.x, local),
        y: lerp(start.y, end.y, local),
        z: lerp(start.z, end.z, local),
      }),
    );
  });
}

export function generatePaletteFromLocked({
  colors,
  locked,
  mode,
}: {
  colors: string[];
  locked: boolean[];
  mode: PaletteMode;
}) {
  const unlockedCount = colors.filter((_, index) => !locked[index]).length;
  const generated = generatePalette(mode, unlockedCount);
  let generatedIndex = 0;
  return colors.map((color, index) => {
    if (locked[index]) return color;
    const next = generated[generatedIndex] ?? color;
    generatedIndex += 1;
    return next;
  });
}

export const varyPalette = (colors: string[]) =>
  colors.map((color) => {
    const current = hexHsl(color);
    return hslHex({
      h: (current.h + random(-12, 12) + 360) % 360,
      l: clamp(current.l + random(-0.05, 0.05), 0.04, 0.97),
      s: clamp(current.s + random(-0.06, 0.06), 0, 1),
    });
  });
