// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { clsx } from "clsx";
import { useCallback, useEffect, useRef, useState } from "react";

import { cancelRuler } from "./api";
import { snapBounds } from "./bounds-snap";
import { Axis } from "./gradient-field";
import { hoveredPixelAt } from "./hovered-pixel";
import { Bounds, PixelSnapshot, Point } from "./pixel-analysis";
import {
  CursorReadout,
  RulerCrosshair,
  ToleranceIndicator,
} from "./ruler-cursor-overlays";
import { GuideLayer } from "./ruler-guide-layer";
import { rulerPointerHandlers } from "./ruler-pointer";
import { PreviewProbeLayer } from "./ruler-preview-probes";
import { Measurement } from "./ruler-types";
import { rulerViewportSize } from "./ruler-viewport-size";
import { RulerWorld } from "./ruler-world";
import { useBoxDrag } from "./use-box-drag";
import { useDistanceProbes } from "./use-distance-probes";
import { useGuideMove } from "./use-guide-move";
import { useLabelHandles } from "./use-label-handles";
import { useOptionKey } from "./use-option-key";
import { useRulerAnalysis } from "./use-ruler-analysis";
import { useRulerClipboard } from "./use-ruler-clipboard";
import { selectedFromLabel, useRulerDeletion } from "./use-ruler-deletion";
import { useRulerGuides } from "./use-ruler-guides";
import { useRecordSlot, useRulerHistory } from "./use-ruler-history";
import { useRulerHotkeys } from "./use-ruler-hotkeys";
import { useRulerScreenshotMode } from "./use-ruler-screenshot-mode";
import { useRulerTolerance } from "./use-ruler-tolerance";
import { useRulerViewToggles } from "./use-ruler-view-toggles";
import { useRulerViewport } from "./use-ruler-viewport";
import { settleWorthwhile } from "./use-settle-animation";
import { useWindowFocus } from "./use-window-focus";

const monitorId = Number(
  new URLSearchParams(window.location.search).get("monitorId") ?? 0,
);

export function RulerWindow() {
  const [snapshot, setSnapshot] = useState<PixelSnapshot>();
  const [screenCursor, setScreenCursor] = useState<Point>();
  const [measurements, setMeasurements] = useState<Measurement[]>([]);
  const view = useRulerViewToggles();
  const windowFocused = useWindowFocus();
  const nextIdRef = useRef(1);
  const cursorRef = useRef<Point | undefined>(undefined);
  const rulerViewport = useRulerViewport();
  const screenshotMode = useRulerScreenshotMode();
  const {
    cycle: cycleTolerance,
    notice: toleranceNotice,
    threshold,
  } = useRulerTolerance();
  const { boxes, field } = useRulerAnalysis({ monitorId, threshold });
  const cursor = screenCursor ? rulerViewport.toWorld(screenCursor) : undefined;
  useEffect(() => {
    cursorRef.current = cursor;
  }, [cursor]);

  const close = useCallback(() => {
    void cancelRuler();
  }, []);

  const { copied, copyColor, copyLatestMeasurement } = useRulerClipboard({
    cursorRef,
    measurements,
    snapshot,
  });

  const { fill, record } = useRecordSlot();
  const deleteLatestMeasurement = useCallback(() => {
    if (measurements.length === 0) return;
    record();
    setMeasurements((current) => current.slice(0, -1));
  }, [measurements.length, record]);

  const guideApi = useRulerGuides({
    field,
    threshold,
    zoom: rulerViewport.zoom,
  });
  const { guides, move: moveGuide, place, previewAt } = guideApi;
  // Alt/Option makes the probes stop at committed guides and box edges too.
  const altHeld = useOptionKey();
  const distanceProbes = useDistanceProbes({
    artifacts: altHeld ? { guides, measurements } : undefined,
    cursor,
    field,
    threshold,
    viewport: rulerViewportSize(),
  });
  const persistDistanceProbe = distanceProbes.persist;
  const stampProbe = useCallback(
    (axis: Axis) => {
      if (!cursorRef.current) return;
      record();
      persistDistanceProbe(axis, cursorRef.current);
    },
    [persistDistanceProbe, record],
  );
  const labels = useLabelHandles(rulerViewport.toWorld, record);
  const { handles, hovered } = labels;
  const guideMove = useGuideMove();
  const { redo, undo } = useRulerHistory({
    fill,
    guides: guideApi,
    labels,
    measurements,
    probes: distanceProbes,
    setMeasurements,
  });
  const { deleteHovered, selectLine } = useRulerDeletion({
    clearHover: labels.clearHover,
    guides,
    hovered: labels.hovered,
    measurements,
    probes: distanceProbes.probes,
    record,
    removeGuide: guideApi.remove,
    removeProbe: distanceProbes.remove,
    setMeasurements,
    zoom: rulerViewport.zoom,
  });
  const { guideAxis } = useRulerHotkeys({
    close,
    copyColor,
    copyLatestMeasurement,
    cycleTolerance,
    deleteHovered,
    deleteLatestMeasurement,
    redo,
    stampProbe,
    toggleCenterlines: view.toggleCenterlines,
    toggleCrosshair: view.toggleCrosshair,
    toggleDetectedBoxes: view.toggleDetectedBoxes,
    undo,
  });
  const commitBox = useCallback(
    (raw: Bounds) => {
      if (!field || (raw.width < 2 && raw.height < 2)) return;
      const snapped = snapBounds({
        bounds: raw,
        boxes,
        field,
        threshold,
        viewport: rulerViewportSize(),
      });
      const from = settleWorthwhile(raw, snapped) ? raw : undefined;
      record();
      setMeasurements((current) => [
        ...current,
        { ...snapped, from, id: nextIdRef.current++ },
      ]);
    },
    [boxes, field, record, threshold],
  );
  const boxDrag = useBoxDrag(commitBox);

  const { hoveredColor } = hoveredPixelAt({
    cursor,
    snapshot,
    viewport: rulerViewportSize(),
  });
  const deviceScale = snapshot ? snapshot.width / window.innerWidth : 1;
  const guidePreview =
    guideAxis && cursor ? previewAt(guideAxis, cursor) : undefined;
  // A hovered chip owns the pointer: transient readouts step aside and the
  // native move cursor has to become visible again.
  const quiet = screenshotMode || hovered !== undefined;
  // Carrying a guide behaves exactly like placing one, gates included.
  const carrying = guideMove.activeId !== undefined;
  // Nearest line within a few screen px: pulsing halo + delete-key target.
  const selected = selectLine({
    active: !quiet && !guideAxis && !carrying && !boxDrag.draft,
    cursor,
  });
  // Hovering a label halos its owner too, previewing what delete removes.
  const highlighted = hovered ? selectedFromLabel(hovered) : selected;
  // Halos and unfocused windows (stale by definition) silence the transients.
  const calm = quiet || carrying || selected !== undefined || !windowFocused;

  const { begin, cancel, finish, move } = rulerPointerHandlers({
    boxDrag,
    guideAxis,
    guideMove,
    moveGuide,
    place,
    record,
    selected,
    setScreenCursor,
    viewport: rulerViewport,
  });

  return (
    <main
      className={clsx(
        "relative h-screen w-screen overflow-hidden select-none",
        // Guide placement shows the native crosshair: the preview line sits at
        // the SNAPPED position, so the true cursor spot must stay visible.
        hovered === undefined &&
          (guideAxis || carrying
            ? "cursor-crosshair! [&_*]:cursor-crosshair!"
            : "cursor-none! [&_*]:cursor-none!"),
      )}
      onDoubleClick={() => {
        if (!guideAxis && !boxDrag.isActive() && !rulerViewport.isPanning())
          rulerViewport.reset();
      }}
      onPointerCancel={cancel}
      onPointerDown={begin}
      onPointerMove={move}
      onPointerUp={finish}
      onWheel={rulerViewport.onWheel}
    >
      <RulerWorld
        boxes={boxes}
        centerlines={view.centerlines}
        detectedBoxes={view.detectedBoxes}
        deviceScale={deviceScale}
        distanceProbes={distanceProbes.probes}
        draft={screenshotMode ? undefined : boxDrag.draft}
        handles={handles}
        highlighted={highlighted}
        measurements={measurements}
        monitorId={monitorId}
        onLoad={setSnapshot}
        style={rulerViewport.style}
      />

      {!calm && view.crosshair && screenCursor ? (
        <RulerCrosshair cursor={screenCursor} />
      ) : null}

      {/* Screen-space cursor furniture: crisp at any zoom of the world. */}
      <PreviewProbeLayer
        probes={
          calm || boxDrag.draft || guideAxis ? [] : distanceProbes.previews
        }
        toScreen={rulerViewport.toScreen}
      />

      {/* Guides paint above the crosshair; the info chips below stay on top. */}
      <GuideLayer
        guides={guides}
        handles={handles}
        preview={quiet ? undefined : guidePreview}
        selectedId={highlighted?.kind === "guide" ? highlighted.id : undefined}
        style={rulerViewport.style}
        viewport={rulerViewportSize()}
      />

      {!screenshotMode && screenCursor && toleranceNotice ? (
        <ToleranceIndicator cursor={screenCursor} tolerance={toleranceNotice} />
      ) : null}
      {!calm && !guideAxis && screenCursor && !toleranceNotice ? (
        <CursorReadout
          copied={copied}
          cursor={screenCursor}
          draft={boxDrag.draft}
          hex={hoveredColor?.hex}
          probes={boxDrag.draft ? [] : distanceProbes.previews}
        />
      ) : null}
    </main>
  );
}
