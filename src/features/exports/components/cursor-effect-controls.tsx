// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Checkbox } from "../../../components/base/checkbox/checkbox";
import { Slider } from "../../../components/base/slider/slider";
import { CursorEffectSettings } from "../types";

export function CursorEffectControls({
  isSaving,
  onChange,
  settings,
}: {
  isSaving: boolean;
  settings: CursorEffectSettings;
  onChange?: (settings: CursorEffectSettings) => void;
}) {
  const sizePercent = Number.isFinite(settings.sizePercent)
    ? settings.sizePercent
    : 100;
  const update = (change: Partial<CursorEffectSettings>) => {
    onChange?.({ ...settings, ...change });
  };
  return (
    <div className="flex flex-col gap-4">
      <Checkbox
        isDisabled={isSaving}
        isSelected={settings.bake}
        onChange={(bake) => {
          update({ bake });
        }}
        size="sm"
      >
        <span className="flex flex-col">
          <span className="text-xs">Bake cursor into recording</span>
          <span className="text-xxs text-muted">
            Dynamic Orbit Capture cursor
          </span>
        </span>
      </Checkbox>
      <Checkbox
        isDisabled={isSaving || !settings.bake}
        isSelected={settings.clipAtVideoEdge}
        onChange={(clipAtVideoEdge) => {
          update({ clipAtVideoEdge });
        }}
        size="sm"
      >
        <span className="text-xs">Clip at video edge</span>
      </Checkbox>
      <Checkbox
        isDisabled={isSaving || !settings.bake}
        isSelected={settings.smoothMovement}
        onChange={(smoothMovement) => {
          update({ smoothMovement });
        }}
        size="sm"
      >
        <span className="flex flex-col">
          <span className="text-xs">Smooth movement</span>
          <span className="text-xxs text-muted">
            Adds natural smoothing and momentum.
          </span>
        </span>
      </Checkbox>
      <Slider
        isDisabled={isSaving || !settings.bake}
        label="Cursor size"
        maxValue={500}
        minValue={50}
        onChange={(nextSizePercent) => {
          update({ sizePercent: nextSizePercent });
        }}
        renderValue={(value) => `${value.toString()}%`}
        step={5}
        value={sizePercent}
      />
      <Checkbox
        isDisabled={isSaving || !settings.bake}
        isSelected={settings.motionBlur}
        onChange={(motionBlur) => {
          update({ motionBlur });
        }}
        size="sm"
      >
        <span className="text-xs">Motion blur</span>
      </Checkbox>
      <Checkbox
        isDisabled={isSaving || !settings.bake}
        isSelected={settings.clickAnimation}
        onChange={(clickAnimation) => {
          update({ clickAnimation });
        }}
        size="sm"
      >
        <span className="text-xs">Click animation</span>
      </Checkbox>
    </div>
  );
}
