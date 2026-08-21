// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  PointerEvent as ReactPointerEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";

import { Point } from "./pixel-analysis";

const ORIGIN: Point = { x: 0, y: 0 };

export type LabelHandles = {
  beginDrag: (key: string, event: ReactPointerEvent<SVGGElement>) => void;
  drag: (event: ReactPointerEvent<SVGGElement>) => void;
  endDrag: (event: ReactPointerEvent<SVGGElement>) => void;
  enter: (key: string) => void;
  leave: (key: string) => void;
  offset: (key: string) => Point;
};

/**
 * Turns persisted label chips into handles: hovering one arms the delete key
 * and reveals the native cursor, dragging one nudges it in world coordinates.
 * Offsets live for the session only — stale keys after a delete cost nothing.
 */
export function useLabelHandles(
  toWorld: (point: Point) => Point,
  record: () => void,
) {
  const [hovered, setHovered] = useState<string>();
  const [offsets, setOffsets] = useState<Record<string, Point>>({});
  const dragRef = useRef<{
    base: Point;
    key: string;
    origin: Point;
    recorded: boolean;
  } | null>(null);
  const offsetsRef = useRef(offsets);
  const toWorldRef = useRef(toWorld);
  useEffect(() => {
    offsetsRef.current = offsets;
  }, [offsets]);
  useEffect(() => {
    toWorldRef.current = toWorld;
  }, [toWorld]);

  const beginDrag = useCallback(
    (key: string, event: ReactPointerEvent<SVGGElement>) => {
      if (event.button !== 0) return;
      // The world underneath must not start a box drag or a pan.
      event.stopPropagation();
      event.preventDefault();
      event.currentTarget.setPointerCapture(event.pointerId);
      dragRef.current = {
        base: offsetsRef.current[key] ?? ORIGIN,
        key,
        origin: toWorldRef.current({ x: event.clientX, y: event.clientY }),
        recorded: false,
      };
      setHovered(key);
    },
    [],
  );

  const drag = useCallback(
    (event: ReactPointerEvent<SVGGElement>) => {
      const gesture = dragRef.current;
      if (!gesture) return;
      event.stopPropagation();
      // History records on the FIRST movement, when a mutation is certain — a
      // bare click must neither add an undo step nor clear the redo stack.
      if (!gesture.recorded) {
        gesture.recorded = true;
        record();
      }
      const point = toWorldRef.current({ x: event.clientX, y: event.clientY });
      setOffsets((current) => ({
        ...current,
        [gesture.key]: {
          x: gesture.base.x + point.x - gesture.origin.x,
          y: gesture.base.y + point.y - gesture.origin.y,
        },
      }));
    },
    [record],
  );

  const endDrag = useCallback((event: ReactPointerEvent<SVGGElement>) => {
    if (!dragRef.current) return;
    dragRef.current = null;
    event.stopPropagation();
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
  }, []);

  const enter = useCallback((key: string) => {
    setHovered(key);
  }, []);

  const leave = useCallback((key: string) => {
    if (dragRef.current) return;
    setHovered((current) => (current === key ? undefined : current));
  }, []);

  const offset = useCallback(
    (key: string) => offsets[key] ?? ORIGIN,
    [offsets],
  );

  /** Deleting a hovered label unmounts it, so no pointerleave ever fires. */
  const clearHover = useCallback(() => {
    dragRef.current = null;
    setHovered(undefined);
  }, []);

  const restore = useCallback((next: Record<string, Point>) => {
    setOffsets(next);
  }, []);

  return {
    clearHover,
    handles: { beginDrag, drag, endDrag, enter, leave, offset },
    hovered,
    offsets,
    restore,
  };
}
