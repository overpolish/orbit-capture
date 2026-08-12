// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Shuffle } from "lucide-react";

import { Button } from "../../../components/base/button/button";
import { ColorPaletteGenerator } from "../../../components/base/input-fields/color-palette-generator";
import { randomMeshComposition } from "../screenshot-background";
import { ScreenshotOutputSettings } from "../screenshot-output";

export function MeshBackgroundControl({
  isDisabled,
  onChange,
  settings,
}: {
  settings: ScreenshotOutputSettings;
  isDisabled?: boolean;
  onChange?: (settings: ScreenshotOutputSettings) => void;
}) {
  const locked = settings.meshLockedColors;
  const preserveLocked = (meshColors: string[]) =>
    meshColors.map((color, index) =>
      locked[index] ? (settings.meshColors[index] ?? color) : color,
    );

  return (
    <ColorPaletteGenerator
      colors={settings.meshColors}
      endContent={
        <Button
          aria-label="Randomize mesh"
          isDisabled={isDisabled}
          onPress={() => {
            const hasLockedColors = locked.some((isLocked) => isLocked);
            const composition = randomMeshComposition(
              hasLockedColors ? settings.meshColors.length : undefined,
            );
            onChange?.({
              ...settings,
              ...composition,
              meshColors: preserveLocked(composition.meshColors),
              meshLockedColors: composition.meshColors.map(
                (_, index) => locked[index] ?? false,
              ),
            });
          }}
          size="sm"
          variant="ghost"
        >
          <Shuffle size={14} />
          Randomize
        </Button>
      }
      isDisabled={isDisabled}
      locked={locked}
      onChange={(meshColors) => {
        onChange?.({ ...settings, meshColors });
      }}
      onLockedChange={(meshLockedColors) => {
        onChange?.({ ...settings, meshLockedColors });
      }}
    />
  );
}
