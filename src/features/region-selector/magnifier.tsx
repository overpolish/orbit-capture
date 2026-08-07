import { AnimatePresence, motion } from "motion/react";
import { RefObject, useEffect, useRef, useState } from "react";

import { MonitorDetails } from "../recording-sources/types";

import { Boundary } from "./boundary";
import { ResizeDirection } from "./types";

type RegionRect = { height: number; width: number; x: number; y: number };

type MagnifierProps = {
  activeHandle: RefObject<HTMLElement | null>;
  monitor: MonitorDetails;
  regionRect: RegionRect;
  resizeDirection: ResizeDirection | undefined;
  screenshot: ArrayBuffer;
};

const CROP_SIZE = 40;
const ZOOM_FACTOR = 5;

export function Magnifier({
  activeHandle,
  monitor,
  regionRect,
  resizeDirection,
  screenshot,
}: MagnifierProps) {
  const sourceCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const magnifierCanvasRef = useRef<HTMLCanvasElement>(null);
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

  useEffect(() => {
    if (!resizeDirection || !activeHandle.current) return;
    const destination = magnifierCanvasRef.current;
    const source = sourceCanvasRef.current;
    const context = destination?.getContext("2d");
    if (!destination || !source || !context) return;

    const outputSize = CROP_SIZE * ZOOM_FACTOR;
    destination.width = outputSize;
    destination.height = outputSize;
    context.imageSmoothingEnabled = false;
    context.clearRect(0, 0, outputSize, outputSize);
    context.drawImage(
      source,
      position.x * monitor.scaleFactor - CROP_SIZE / 2,
      position.y * monitor.scaleFactor - CROP_SIZE / 2,
      CROP_SIZE,
      CROP_SIZE,
      0,
      0,
      outputSize,
      outputSize,
    );
  }, [activeHandle, monitor.scaleFactor, position, resizeDirection]);

  return (
    <AnimatePresence>
      {resizeDirection ? (
        <motion.div
          animate={{ opacity: 1, scale: 1 }}
          className="pointer-events-none fixed overflow-hidden rounded-sm border border-content-fg/10 shadow-md"
          exit={{ opacity: 0, scale: 0 }}
          initial={{ opacity: 0, scale: 0, x: "-50%", y: "-50%" }}
          style={{ left: position.x, top: position.y }}
        >
          <canvas
            className="aspect-square max-h-[100px] max-w-[100px]"
            ref={magnifierCanvasRef}
          />
          <Boundary direction={resizeDirection} />
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}
