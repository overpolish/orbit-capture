// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  MouseEvent as ReactMouseEvent,
  RefObject,
  useEffect,
  useState,
} from "react";

import {
  ScreenshotOutputSettings,
  screenshotLayout,
} from "../screenshot-output";

import { ScreenshotLayoutControl } from "./screenshot-layout-control";
import { ScreenshotRadiusControl } from "./screenshot-radius-control";

import type { ScreenshotLayoutChange } from "./screenshot-layout-control";

/// The composed image itself is drawn by the platform preview path. This layer
/// only carries the on-screen controls that sit above it.
export function ScreenshotPreviewLayer({
  isCropTarget = false,
  isEditing,
  isItemSelected = false,
  isSelecting = false,
  onItemContextMenu,
  onItemSelect,
  onLayoutChange,
  onLayoutInteractionEnd,
  onLayoutInteractionStart,
  onOutputChange,
  onRadiusChange,
  onRadiusChangeEnd,
  output,
  outputRef,
  previewCanvasRef,
  previewUrl,
  radiusPercent,
  settings,
  snapFrames,
  source,
}: {
  isEditing: boolean;
  output: { height: number; width: number };
  outputRef: RefObject<HTMLDivElement | null>;
  radiusPercent: number;
  settings: ScreenshotOutputSettings;
  source: { height: number; width: number };
  isCropTarget?: boolean;
  isItemSelected?: boolean;
  isSelecting?: boolean;
  onItemContextMenu?: (event: ReactMouseEvent<HTMLDivElement>) => void;
  onItemSelect?: () => void;
  onLayoutChange?: (
    change: ScreenshotLayoutChange,
  ) => ScreenshotOutputSettings | undefined;
  onLayoutInteractionEnd?: () => void;
  onLayoutInteractionStart?: () => void;
  onOutputChange?: (settings: ScreenshotOutputSettings) => void;
  onRadiusChange?: (radiusPercent: number) => void;
  onRadiusChangeEnd?: () => void;
  previewCanvasRef?: RefObject<HTMLCanvasElement | null>;
  previewUrl?: string | null;
  snapFrames?: { height: number; width: number; x: number; y: number }[];
}) {
  const [draft, setDraft] = useState(settings);
  useEffect(() => {
    // Inspector changes replace the committed layout after a gesture ends.
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setDraft(settings);
  }, [settings]);
  const placement = screenshotLayout(source, output, draft);

  if (!isEditing && !isCropTarget && !isSelecting) return null;
  return (
    <>
      {isSelecting ? (
        <ScreenshotLayoutControl
          controlsVisible={isItemSelected}
          mediaRef={outputRef}
          mode="transform"
          onChange={(change) => {
            const next = onLayoutChange?.(change) ?? change.settings;
            setDraft(next);
            // An auto-fitting canvas commits the whole workspace through
            // `onLayoutChange` instead. Without such a handler nothing else
            // commits, so the alt drag has to land here like any other.
            if (!change.autoFitCanvas || !onLayoutChange)
              onOutputChange?.(next);
            return next;
          }}
          onInteractionEnd={onLayoutInteractionEnd}
          onInteractionStart={() => {
            onItemSelect?.();
            onLayoutInteractionStart?.();
          }}
          onItemContextMenu={onItemContextMenu}
          output={output}
          previewSourceRef={previewCanvasRef}
          previewUrl={previewUrl}
          settings={draft}
          snapFrames={snapFrames}
          source={source}
        />
      ) : null}
      {isEditing || isCropTarget ? (
        <>
          <ScreenshotLayoutControl
            controlsVisible={isEditing}
            mediaRef={outputRef}
            mode="crop"
            onChange={(change) => {
              const next = change.settings;
              setDraft(next);
              onOutputChange?.(next);
              return next;
            }}
            onInteractionStart={
              isCropTarget ? () => onItemSelect?.() : undefined
            }
            onItemContextMenu={onItemContextMenu}
            output={output}
            previewSourceRef={previewCanvasRef}
            previewUrl={previewUrl}
            settings={draft}
            source={source}
          />
          {isEditing ? (
            <ScreenshotRadiusControl
              canvasWidth={output.width}
              height={placement.crop.height}
              mediaRef={outputRef}
              onChange={(nextRadiusPercent) => {
                const next = { ...draft, radiusPercent: nextRadiusPercent };
                setDraft(next);
                onOutputChange?.(next);
                onRadiusChange?.(nextRadiusPercent);
              }}
              onChangeEnd={onRadiusChangeEnd}
              placementOffset={{
                x: placement.crop.x,
                y: placement.crop.y,
              }}
              radiusPercent={radiusPercent}
              width={placement.crop.width}
            />
          ) : null}
        </>
      ) : null}
    </>
  );
}
