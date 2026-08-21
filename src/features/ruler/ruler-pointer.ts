// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Dispatch, PointerEvent, SetStateAction } from "react";

import type { Axis } from "./gradient-field";
import type { Point } from "./pixel-analysis";
import type { useBoxDrag } from "./use-box-drag";
import type { useGuideMove } from "./use-guide-move";
import type { SelectedLine } from "./use-ruler-deletion";
import type { useRulerViewport } from "./use-ruler-viewport";

/**
 * The world surface's pointer gestures, in priority order: a pan claims the
 * event first, a held guide axis stamps a guide, a haloed guide is picked up and
 * carried, otherwise it is a box drag.
 */
export function rulerPointerHandlers({
  boxDrag,
  guideAxis,
  guideMove,
  moveGuide,
  place,
  record,
  selected,
  setScreenCursor,
  viewport,
}: {
  boxDrag: ReturnType<typeof useBoxDrag>;
  guideMove: ReturnType<typeof useGuideMove>;
  moveGuide: (id: number, point: Point) => void;
  place: (axis: Axis, point: Point) => void;
  record: () => void;
  setScreenCursor: Dispatch<SetStateAction<Point | undefined>>;
  viewport: ReturnType<typeof useRulerViewport>;
  guideAxis?: Axis;
  selected?: SelectedLine;
}) {
  const move = (event: PointerEvent<HTMLElement>) => {
    const screenPoint = { x: event.clientX, y: event.clientY };
    setScreenCursor(screenPoint);
    if (viewport.movePan(event)) return;
    const point = viewport.toWorld(screenPoint);
    const gesture = guideMove.gesture();
    if (gesture) {
      // History records on the FIRST movement, when a mutation is certain — a
      // bare click on a haloed guide must not add an undo step.
      if (!gesture.recorded) {
        gesture.recorded = true;
        record();
      }
      moveGuide(gesture.id, point);
      return;
    }
    boxDrag.move(screenPoint, point);
  };

  const begin = (event: PointerEvent<HTMLElement>) => {
    if (viewport.beginPan(event)) return;
    if (event.button !== 0) return;
    const screenPoint = { x: event.clientX, y: event.clientY };
    setScreenCursor(screenPoint);
    const point = viewport.toWorld(screenPoint);
    if (guideAxis) {
      record();
      place(guideAxis, point);
      return;
    }
    event.currentTarget.setPointerCapture(event.pointerId);
    if (selected?.kind === "guide") {
      guideMove.begin(selected.id);
      return;
    }
    boxDrag.begin(screenPoint, point);
  };

  const finish = (event: PointerEvent<HTMLElement>) => {
    if (viewport.endPan(event)) return;
    if (guideMove.end()) return;
    boxDrag.finish(viewport.toWorld({ x: event.clientX, y: event.clientY }));
  };

  const cancel = () => {
    guideMove.end();
    boxDrag.cancel();
  };

  return { begin, cancel, finish, move };
}
