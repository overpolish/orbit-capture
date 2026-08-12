// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ImageIcon } from "lucide-react";
import { RefObject, useEffect, useState } from "react";

import {
  ScreenshotOutputSettings,
  screenshotLayout,
} from "../screenshot-output";

import { ScreenshotLayoutControl } from "./screenshot-layout-control";
import { ScreenshotRadiusControl } from "./screenshot-radius-control";

export function ScreenshotPreviewLayer({
  alt,
  canvasRadius,
  isEditing,
  onOutputChange,
  onRadiusChange,
  onRadiusChangeEnd,
  onReady,
  output,
  outputRef,
  previewUrl,
  radiusPercent,
  settings,
  source,
}: {
  alt: string;
  canvasRadius: number;
  isEditing: boolean;
  onReady: () => void;
  output: { height: number; width: number };
  outputRef: RefObject<HTMLDivElement | null>;
  radiusPercent: number;
  settings: ScreenshotOutputSettings;
  source: { height: number; width: number };
  onOutputChange?: (settings: ScreenshotOutputSettings) => void;
  onRadiusChange?: (radiusPercent: number) => void;
  onRadiusChangeEnd?: () => void;
  previewUrl?: string | null;
}) {
  const [draft, setDraft] = useState(settings);
  useEffect(() => {
    // Inspector changes replace the committed layout after a gesture ends.
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setDraft(settings);
  }, [settings]);
  const placement = screenshotLayout(source, output, draft);
  const radius =
    (Math.min(placement.crop.width, placement.crop.height) * radiusPercent) /
    100;
  const shadowSigma = Math.min(
    48,
    Math.max(6, Math.min(placement.crop.width, placement.crop.height) * 0.018),
  );
  // WebKit antialiases transformed sibling edges independently. When the
  // screenshot crop ends exactly on the output edge, a fractional viewer zoom
  // can therefore expose a one-device-pixel line of the background between
  // them. Overlap only those coincident edges by one displayed pixel; the
  // output clip removes the bleed and the actual crop geometry stays exact.
  const edgeBleed = "calc(1px * var(--preview-inverse-scale, 1))";
  const touchesLeft = Math.abs(placement.crop.x) < 0.01;
  const touchesTop = Math.abs(placement.crop.y) < 0.01;
  const touchesRight =
    Math.abs(placement.crop.x + placement.crop.width - output.width) < 0.01;
  const touchesBottom =
    Math.abs(placement.crop.y + placement.crop.height - output.height) < 0.01;
  const cropLeft = touchesLeft
    ? `calc(${placement.crop.x.toString()}px - ${edgeBleed})`
    : `${placement.crop.x.toString()}px`;
  const cropTop = touchesTop
    ? `calc(${placement.crop.y.toString()}px - ${edgeBleed})`
    : `${placement.crop.y.toString()}px`;
  const cropWidth = `calc(${placement.crop.width.toString()}px${touchesLeft ? ` + ${edgeBleed}` : ""}${touchesRight ? ` + ${edgeBleed}` : ""})`;
  const cropHeight = `calc(${placement.crop.height.toString()}px${touchesTop ? ` + ${edgeBleed}` : ""}${touchesBottom ? ` + ${edgeBleed}` : ""})`;
  const imageLeft = `calc(${(placement.image.x - placement.crop.x).toString()}px${touchesLeft ? ` + ${edgeBleed}` : ""})`;
  const imageTop = `calc(${(placement.image.y - placement.crop.y).toString()}px${touchesTop ? ` + ${edgeBleed}` : ""})`;

  return (
    <>
      <div
        className="pointer-events-none absolute inset-0 overflow-hidden"
        style={{ clipPath: `inset(0 round ${canvasRadius.toString()}px)` }}
      >
        <div
          className="absolute overflow-hidden"
          style={{
            borderRadius: `${radius.toString()}px`,
            boxShadow: draft.dropShadow
              ? `0 ${(shadowSigma * 0.6).toString()}px ${(shadowSigma * 2).toString()}px rgb(0 0 0 / 35%)`
              : "none",
            height: cropHeight,
            left: cropLeft,
            top: cropTop,
            width: cropWidth,
          }}
        >
          {previewUrl ? (
            <img
              alt={alt}
              className="absolute max-w-none"
              draggable={false}
              onLoad={onReady}
              src={previewUrl}
              style={{
                height: `${placement.image.height.toString()}px`,
                left: imageLeft,
                top: imageTop,
                width: `${placement.image.width.toString()}px`,
              }}
            />
          ) : (
            <ImageIcon className="text-muted/50" size={40} />
          )}
        </div>
      </div>
      {isEditing ? (
        <>
          <ScreenshotLayoutControl
            mediaRef={outputRef}
            onChange={setDraft}
            onChangeEnd={onOutputChange}
            output={output}
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
      ) : null}
    </>
  );
}
