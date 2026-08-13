// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RefObject, useEffect, useState } from "react";

import {
  ScreenshotOutputSettings,
  screenshotLayout,
} from "../screenshot-output";

import { ScreenshotLayoutControl } from "./screenshot-layout-control";
import { ScreenshotRadiusControl } from "./screenshot-radius-control";

/// The composed image itself is drawn by the platform preview path — a native
/// pane on macOS, the CPU compositor blitted into a canvas elsewhere. This
/// layer only carries the on-screen controls that sit above it.
export function ScreenshotPreviewLayer({
  isEditing,
  onOutputChange,
  onRadiusChange,
  onRadiusChangeEnd,
  output,
  outputRef,
  previewCanvasRef,
  previewUrl,
  radiusPercent,
  settings,
  source,
}: {
  isEditing: boolean;
  output: { height: number; width: number };
  outputRef: RefObject<HTMLDivElement | null>;
  radiusPercent: number;
  settings: ScreenshotOutputSettings;
  source: { height: number; width: number };
  onOutputChange?: (settings: ScreenshotOutputSettings) => void;
  onRadiusChange?: (radiusPercent: number) => void;
  onRadiusChangeEnd?: () => void;
  previewCanvasRef?: RefObject<HTMLCanvasElement | null>;
  previewUrl?: string | null;
}) {
  const [draft, setDraft] = useState(settings);
  useEffect(() => {
    // Inspector changes replace the committed layout after a gesture ends.
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setDraft(settings);
  }, [settings]);
  const placement = screenshotLayout(source, output, draft);

  if (!isEditing) return null;
  return (
    <>
      <ScreenshotLayoutControl
        mediaRef={outputRef}
        onChange={(next) => {
          setDraft(next);
          onOutputChange?.(next);
        }}
        output={output}
        previewSourceRef={previewCanvasRef}
        previewUrl={previewUrl}
        settings={draft}
        source={source}
      />
      <ScreenshotRadiusControl
        canvasWidth={output.width}
        height={placement.crop.height}
        mediaRef={outputRef}
        onChange={onRadiusChange}
        onChangeEnd={onRadiusChangeEnd}
        placementOffset={{
          x: placement.crop.x,
          y: placement.crop.y,
        }}
        radiusPercent={radiusPercent}
        width={placement.crop.width}
      />
    </>
  );
}
