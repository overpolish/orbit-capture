// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { motion } from "motion/react";
import { CSSProperties, useEffect, useRef } from "react";

export type PixelMagnifierDirection =
  | "bottom"
  | "bottomLeft"
  | "bottomRight"
  | "left"
  | "right"
  | "top"
  | "topLeft"
  | "topRight";

const CROP_SIZE = 40;
const ZOOM_FACTOR = 5;

const boundaryMap: Record<
  PixelMagnifierDirection,
  { rotation: number; type: "corner" | "edge" }
> = {
  bottom: { rotation: 180, type: "edge" },
  bottomLeft: { rotation: 270, type: "corner" },
  bottomRight: { rotation: 180, type: "corner" },
  left: { rotation: 270, type: "edge" },
  right: { rotation: 90, type: "edge" },
  top: { rotation: 0, type: "edge" },
  topLeft: { rotation: 0, type: "corner" },
  topRight: { rotation: 90, type: "corner" },
};

function MagnifierBoundary({
  direction,
}: {
  direction: PixelMagnifierDirection;
}) {
  const { rotation, type } = boundaryMap[direction];

  return (
    <svg
      aria-hidden
      className="absolute inset-0"
      style={{ transform: `rotate(${String(rotation)}deg)` }}
      viewBox="0 0 100 100"
    >
      {type === "edge" ? (
        <rect className="fill-content-fg/10" height="50" width="100" />
      ) : (
        <path
          className="fill-content-fg/10"
          d="M 0 0 H 100 V 50 H 50 V 100 H 0 Z"
        />
      )}
    </svg>
  );
}

function PixelMagnifier({
  className,
  direction,
  point,
  source,
  style,
}: {
  direction: PixelMagnifierDirection;
  point: { x: number; y: number };
  source: CanvasImageSource | null;
  className?: string;
  style?: CSSProperties;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context || !source) return;
    const outputSize = CROP_SIZE * ZOOM_FACTOR;
    canvas.width = outputSize;
    canvas.height = outputSize;
    context.imageSmoothingEnabled = false;
    context.clearRect(0, 0, outputSize, outputSize);
    context.drawImage(
      source,
      point.x - CROP_SIZE / 2,
      point.y - CROP_SIZE / 2,
      CROP_SIZE,
      CROP_SIZE,
      0,
      0,
      outputSize,
      outputSize,
    );
  }, [point.x, point.y, source]);

  return (
    <div
      className={`pointer-events-none overflow-hidden rounded-sm border border-content-fg/10 shadow-md ${className ?? ""}`}
      style={style}
    >
      <canvas className="block size-full" ref={canvasRef} />
      <MagnifierBoundary direction={direction} />
    </div>
  );
}

export function AnimatedPixelMagnifier({
  className,
  direction,
  point,
  source,
  style,
}: {
  direction: PixelMagnifierDirection;
  point: { x: number; y: number };
  source: CanvasImageSource | null;
  className?: string;
  style?: CSSProperties;
}) {
  return (
    <motion.div
      animate={{ opacity: 1, scale: 1 }}
      className={className}
      exit={{ opacity: 0, scale: 0 }}
      initial={{ opacity: 0, scale: 0, x: "-50%", y: "-50%" }}
      style={style}
    >
      <PixelMagnifier
        className="relative size-full"
        direction={direction}
        point={point}
        source={source}
      />
    </motion.div>
  );
}
