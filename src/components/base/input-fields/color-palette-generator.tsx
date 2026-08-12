// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { CircleDashed, Cloud, Dices, Sun, WandSparkles } from "lucide-react";
import { ReactNode } from "react";
import { TooltipTrigger } from "react-aria-components";

import {
  generatePaletteFromLocked,
  PaletteMode,
  varyPalette,
} from "../../../lib/palette-generator";
import { Button } from "../button/button";
import { Tooltip } from "../tooltip/tooltip";

import { ColorSwatch } from "./color-swatch";

const paletteActions: Array<{
  icon: typeof Sun;
  label: string;
  mode: PaletteMode;
}> = [
  { icon: Sun, label: "Bright palette", mode: "bright" },
  { icon: Cloud, label: "Dull palette", mode: "dull" },
  { icon: CircleDashed, label: "Shades palette", mode: "shades" },
  { icon: Dices, label: "Chaotic palette", mode: "chaotic" },
];

const swatchKeys = ["one", "two", "three", "four", "five"];

export function ColorPaletteGenerator({
  colors,
  endContent,
  isDisabled,
  locked = colors.map(() => false),
  onChange,
  onLockedChange,
}: {
  colors: string[];
  endContent?: ReactNode;
  isDisabled?: boolean;
  locked?: boolean[];
  onChange?: (colors: string[]) => void;
  onLockedChange?: (locked: boolean[]) => void;
}) {
  const preserveLocked = (nextColors: string[]) =>
    nextColors.map((color, index) =>
      locked[index] ? (colors[index] ?? color) : color,
    );

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-3">
        <span className="text-xs text-content-fg">Palette</span>
        <div className="flex items-center gap-1">
          {colors.slice(0, swatchKeys.length).map((color, index) => (
            <ColorSwatch
              ariaLabel={`Palette colour ${(index + 1).toString()}`}
              isDisabled={isDisabled}
              isLocked={locked[index]}
              key={swatchKeys[index]}
              onChange={(value) => {
                onChange?.(
                  colors.map((current, colorIndex) =>
                    colorIndex === index ? value : current,
                  ),
                );
              }}
              onContextMenu={(event) => {
                event.preventDefault();
                if (isDisabled) return;
                onLockedChange?.(
                  colors.map((_, colorIndex) =>
                    colorIndex === index
                      ? !locked[index]
                      : (locked[colorIndex] ?? false),
                  ),
                );
              }}
              value={color}
            />
          ))}
        </div>
      </div>
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-1">
          {paletteActions.map(({ icon: Icon, label, mode }) => (
            <TooltipTrigger delay={400} key={mode}>
              <Button
                aria-label={label}
                icon
                isDisabled={isDisabled}
                onPress={() => {
                  onChange?.(
                    generatePaletteFromLocked({ colors, locked, mode }),
                  );
                }}
                size="sm"
                variant="ghost"
              >
                <Icon size={14} />
              </Button>
              <Tooltip>{label}</Tooltip>
            </TooltipTrigger>
          ))}
          <TooltipTrigger delay={400}>
            <Button
              aria-label="Vary palette"
              icon
              isDisabled={isDisabled}
              onPress={() => {
                onChange?.(preserveLocked(varyPalette(colors)));
              }}
              size="sm"
              variant="ghost"
            >
              <WandSparkles size={14} />
            </Button>
            <Tooltip>Vary palette</Tooltip>
          </TooltipTrigger>
        </div>
        {endContent}
      </div>
      <span className="text-[10px] text-muted">
        Right-click a colour to lock it
      </span>
    </div>
  );
}
