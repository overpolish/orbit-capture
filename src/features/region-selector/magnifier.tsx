// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { AnimatePresence } from "motion/react";
import { useEffect, useRef, useState } from "react";

import { AnimatedPixelMagnifier } from "../../components/shared/canvas-tools/pixel-magnifier";
import { MonitorDetails } from "../recording-sources/types";

import { ResizeDirection } from "./types";

type RegionRect = { height: number; width: number; x: number; y: number };

type MagnifierProps = {
  monitor: MonitorDetails;
  regionRect: RegionRect;
  resizeDirection: ResizeDirection | undefined;
  screenshot: ArrayBuffer;
};

export function Magnifier({
  monitor,
  regionRect,
  resizeDirection,
  screenshot,
}: MagnifierProps) {
  const sourceCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const regionRef = useRef(regionRect);
  const lastPointerRef = useRef<{ x: number; y: number } | null>(null);
  const [position, setPosition] = useState({ x: 0, y: 0 });

  useEffect(() => {
    regionRef.current = regionRect;
  }, [regionRect]);

  useEffect(() => {
    const expectedLength =
      monitor.physicalSize.width * monitor.physicalSize.height * 4;
    if (screenshot.byteLength !== expectedLength) return;

    const canvas = document.createElement("canvas");
    canvas.width = monitor.physicalSize.width;
    canvas.height = monitor.physicalSize.height;
    const context = canvas.getContext("2d");
    if (!context) return;
    context.putImageData(
      new ImageData(
        new Uint8ClampedArray(screenshot),
        monitor.physicalSize.width,
        monitor.physicalSize.height,
      ),
      0,
      0,
    );
    sourceCanvasRef.current = canvas;
  }, [monitor, screenshot]);

  useEffect(() => {
    const update = (event?: MouseEvent) => {
      if (event) {
        lastPointerRef.current = { x: event.clientX, y: event.clientY };
      }
      const rect = regionRef.current;
      const direction = resizeDirection?.toLowerCase();
      let x = rect.x + rect.width / 2;
      let y = rect.y + rect.height / 2;
      const pointer = lastPointerRef.current;

      if (direction?.includes("left")) x = rect.x;
      if (direction?.includes("right")) x = rect.x + rect.width;
      if (direction?.includes("top")) y = rect.y;
      if (direction?.includes("bottom")) y = rect.y + rect.height;
      if (direction === "top" || direction === "bottom") {
        x = pointer?.x ?? x;
      }
      if (direction === "left" || direction === "right") {
        y = pointer?.y ?? y;
      }

      // This also runs from mouse events after the initial handle placement.
      // eslint-disable-next-line @eslint-react/set-state-in-effect
      setPosition({
        x: Math.max(rect.x, Math.min(x, rect.x + rect.width)),
        y: Math.max(rect.y, Math.min(y, rect.y + rect.height)),
      });
    };

    // Position immediately from the active edge/corner before the first move.
    update();
    window.addEventListener("mousemove", update);
    return () => {
      window.removeEventListener("mousemove", update);
    };
  }, [resizeDirection]);

  return (
    <AnimatePresence>
      {resizeDirection ? (
        <AnimatedPixelMagnifier
          className="pointer-events-none fixed"
          direction={resizeDirection}
          point={{
            x: position.x * monitor.scaleFactor,
            y: position.y * monitor.scaleFactor,
          }}
          source={sourceCanvasRef.current}
          style={{
            height: 100,
            left: position.x,
            top: position.y,
            width: 100,
          }}
        />
      ) : null}
    </AnimatePresence>
  );
}
