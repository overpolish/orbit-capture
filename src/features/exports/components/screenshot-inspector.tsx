// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Checkbox } from "../../../components/base/checkbox/checkbox";
import { ColorSwatch } from "../../../components/base/input-fields/color-swatch";
import { OverflowShadow } from "../../../components/base/overflow-shadow/overflow-shadow";
import { PillGroup } from "../../../components/base/pill-group/pill-group";
import { AspectRatio } from "../../../components/shared/aspect-ratio/aspect-ratio";
import {
  resetScreenshotLayout,
  ScreenshotOutputSettings,
} from "../screenshot-output";

import { MeshBackgroundControl } from "./mesh-background-control";

const backgroundTypes = [
  { id: "solid", label: "Solid" },
  { id: "mesh", label: "Mesh" },
];

export function ScreenshotInspector({
  isSaving,
  onChange,
  settings,
  sourceHeight,
  sourceWidth,
}: {
  settings: ScreenshotOutputSettings;
  sourceHeight: number;
  sourceWidth: number;
  isSaving?: boolean;
  onChange?: (settings: ScreenshotOutputSettings) => void;
}) {
  return (
    <aside className="flex min-h-0 min-w-0 flex-col border-r border-muted/15 bg-content/35">
      <OverflowShadow rootClassName="min-h-0 grow" shadowRadius="none">
        <ScreenshotOutputControls
          isSaving={isSaving}
          onChange={onChange}
          settings={settings}
          sourceHeight={sourceHeight}
          sourceWidth={sourceWidth}
        />
      </OverflowShadow>
    </aside>
  );
}

export function ScreenshotOutputControls({
  className = "p-4",
  isSaving,
  onChange,
  settings,
  sourceHeight,
  sourceWidth,
}: {
  settings: ScreenshotOutputSettings;
  sourceHeight: number;
  sourceWidth: number;
  className?: string;
  isSaving?: boolean;
  onChange?: (settings: ScreenshotOutputSettings) => void;
}) {
  const update = (patch: Partial<ScreenshotOutputSettings>) => {
    onChange?.({ ...settings, ...patch });
  };
  const resizeCanvas = (width: number, height: number) => {
    // Canvas dimensions change the coordinate system behind both the crop and
    // placed source. Reframe them together so a previous aspect ratio cannot
    // leave an oversized invisible crop window around the actual clip.
    onChange?.(
      resetScreenshotLayout(
        {
          ...settings,
          height: Math.round(height),
          width: Math.round(width),
        },
        { height: sourceHeight, width: sourceWidth },
      ),
    );
  };

  return (
    <div className={`flex flex-col gap-4 ${className}`}>
      <div className={isSaving ? "pointer-events-none opacity-50" : ""}>
        <AspectRatio
          height={settings.height}
          initialLinked
          layout="stacked"
          onReset={
            settings.width !== sourceWidth || settings.height !== sourceHeight
              ? () => {
                  onChange?.(
                    resetScreenshotLayout(
                      {
                        ...settings,
                        height: sourceHeight,
                        width: sourceWidth,
                      },
                      { height: sourceHeight, width: sourceWidth },
                    ),
                  );
                }
              : undefined
          }
          setDimensions={(width, height) => {
            resizeCanvas(width, height);
          }}
          setHeight={(height) => {
            resizeCanvas(settings.width, height);
          }}
          setWidth={(width) => {
            resizeCanvas(width, settings.height);
          }}
          width={settings.width}
        />
      </div>

      <div className="flex items-center justify-between gap-3">
        <span className="text-xs text-content-fg">Background</span>
        <PillGroup
          ariaLabel="Screenshot background type"
          display="label"
          isDisabled={isSaving}
          items={backgroundTypes}
          onSelectionChange={(backgroundType) => {
            update({
              backgroundType:
                backgroundType as ScreenshotOutputSettings["backgroundType"],
            });
          }}
          selected={settings.backgroundType}
        />
      </div>

      {settings.backgroundType === "solid" ? (
        <div className="flex items-center justify-between gap-3">
          <span className="text-xs text-content-fg">Colour</span>
          <ColorSwatch
            ariaLabel="Background colour"
            isDisabled={isSaving}
            onChange={(backgroundColor) => {
              update({ backgroundColor });
            }}
            value={settings.backgroundColor}
          />
        </div>
      ) : (
        <MeshBackgroundControl
          isDisabled={isSaving}
          onChange={onChange}
          settings={settings}
        />
      )}

      <Checkbox
        isDisabled={isSaving}
        isSelected={settings.dropShadow}
        onChange={(dropShadow) => {
          update({ dropShadow });
        }}
        size="sm"
      >
        <span className="text-xs">Drop shadow</span>
      </Checkbox>
    </div>
  );
}
