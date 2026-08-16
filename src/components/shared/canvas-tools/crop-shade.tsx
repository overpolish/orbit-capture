// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { CanvasRect } from "./selection-frame";

const roundedRectPath = (width: number, height: number, radius: number) => {
  const safeRadius = Math.max(0, Math.min(radius, width / 2, height / 2));
  return [
    `M ${safeRadius.toString()} 0`,
    `H ${(width - safeRadius).toString()}`,
    `A ${safeRadius.toString()} ${safeRadius.toString()} 0 0 1 ${width.toString()} ${safeRadius.toString()}`,
    `V ${(height - safeRadius).toString()}`,
    `A ${safeRadius.toString()} ${safeRadius.toString()} 0 0 1 ${(width - safeRadius).toString()} ${height.toString()}`,
    `H ${safeRadius.toString()}`,
    `A ${safeRadius.toString()} ${safeRadius.toString()} 0 0 1 0 ${(height - safeRadius).toString()}`,
    `V ${safeRadius.toString()}`,
    `A ${safeRadius.toString()} ${safeRadius.toString()} 0 0 1 ${safeRadius.toString()} 0 Z`,
  ].join(" ");
};

export function CropShade({
  crop,
  image,
  radius = 0,
}: {
  crop: CanvasRect;
  image: CanvasRect;
  radius?: number;
}) {
  const regions = [
    {
      height: Math.max(0, crop.y - image.y),
      id: "top",
      left: image.x,
      top: image.y,
      width: image.width,
    },
    {
      height: Math.max(0, image.y + image.height - (crop.y + crop.height)),
      id: "bottom",
      left: image.x,
      top: crop.y + crop.height,
      width: image.width,
    },
    {
      height: crop.height,
      id: "left",
      left: image.x,
      top: crop.y,
      width: Math.max(0, crop.x - image.x),
    },
    {
      height: crop.height,
      id: "right",
      left: crop.x + crop.width,
      top: crop.y,
      width: Math.max(0, image.x + image.width - (crop.x + crop.width)),
    },
  ];

  return (
    <div className="pointer-events-none absolute inset-0 overflow-visible">
      {regions.map(({ id, ...region }) => (
        <div className="absolute bg-black/40" key={id} style={region} />
      ))}
      {radius > 0 && crop.width > 0 && crop.height > 0 ? (
        <svg
          aria-hidden
          className="absolute overflow-visible"
          height={crop.height}
          style={{ left: crop.x, top: crop.y }}
          viewBox={`0 0 ${crop.width.toString()} ${crop.height.toString()}`}
          width={crop.width}
        >
          <path
            d={`M 0 0 H ${crop.width.toString()} V ${crop.height.toString()} H 0 Z ${roundedRectPath(crop.width, crop.height, radius)}`}
            fill="rgb(0 0 0 / 0.4)"
            fillRule="evenodd"
          />
        </svg>
      ) : null}
    </div>
  );
}
