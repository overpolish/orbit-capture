// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  SiInstagram,
  SiTiktok,
  SiYoutube,
} from "@icons-pack/react-simple-icons";
import { Link, Unlink, WandSparkles } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Selection, ToggleButtonGroup } from "react-aria-components";

import { ToggleButton } from "../../base/button/toggle-button";
import { NumberField } from "../../base/input-fields/number-field";
import { CheckOnClickButton } from "../check-on-click-button/check-on-click-button";

/** Simplified whole-number aspect ratio units for width and height. */
type AspectRatioParts = { ratioHeight: number; ratioWidth: number };

/**
 * Returns the greatest common divisor for two numbers.
 */
const greatestCommonDivisor = (a: number, b: number): number => {
  a = Math.abs(a);
  b = Math.abs(b);
  while (b) {
    const temp = b;
    b = a % b;
    a = temp;
  }
  return a || 1;
};

/**
 * Reduces a width/height pair to the simplest whole-number ratio.
 * Example: 1920x1080 -> { ratioWidth: 16, ratioHeight: 9 }
 */
const reduceToRatio = (width: number, height: number): AspectRatioParts => {
  const divisor = greatestCommonDivisor(width, height);
  return {
    ratioHeight: Math.round(height / divisor),
    ratioWidth: Math.round(width / divisor),
  };
};

/**
 * Parses a preset id like "16:9" into an AspectRatioParts object.
 */
const parseRatioFromId = (id: string | null): AspectRatioParts | undefined => {
  if (!id) return undefined;
  const [a, b] = id.split(":").map((n) => Number.parseInt(n, 10));
  if (!Number.isFinite(a) || !Number.isFinite(b) || a <= 0 || b <= 0) {
    return undefined;
  }

  return { ratioHeight: b, ratioWidth: a };
};

const DEBOUNCE_MS = 80; // Gives time for state updates to propagate before calculating adjustments

const numberFieldStyles: React.ComponentProps<typeof NumberField> = {
  centered: true,
  className: "w-15",
  showSteppers: false,
  size: "sm",
  variant: "line",
};

type PlatformIconsProps = {
  onPressInstagram?: () => void;
  onPressTiktok?: () => void;
  onPressYoutube?: () => void;
};
const PlatformIcons = ({
  onPressInstagram,
  onPressTiktok,
  onPressYoutube,
}: PlatformIconsProps) => {
  const styles: React.ComponentProps<typeof CheckOnClickButton> = {
    blur: "xs",
    showFocus: false,
    size: "sm",
    variant: "ghost",
  };

  return (
    <div className="flex flex-row items-center">
      <CheckOnClickButton {...styles} onPress={onPressYoutube}>
        <SiYoutube size={20} />
      </CheckOnClickButton>

      <CheckOnClickButton {...styles} onPress={onPressInstagram}>
        <SiInstagram size={16} />
      </CheckOnClickButton>

      <CheckOnClickButton {...styles} onPress={onPressTiktok}>
        <SiTiktok size={14} />
      </CheckOnClickButton>
    </div>
  );
};

type AspectRatioProps = {
  defaultHeight?: number;
  defaultWidth?: number;
  height?: number;
  initialLinked?: boolean;
  onApply?: (width: number, height: number) => void;
  onRatioChange?: (ratio: number | undefined) => void;
  setHeight?: (value: number) => void;
  setWidth?: (value: number) => void;
  width?: number;
};

export const AspectRatio = ({
  defaultHeight = 1080,
  defaultWidth = 1920,
  height,
  initialLinked = false,
  onApply,
  onRatioChange,
  setHeight,
  setWidth,
  width,
}: AspectRatioProps) => {
  // Determine if fully controlled (both values and setters provided)
  const isControlled =
    width != null && height != null && setWidth != null && setHeight != null;

  // Uncontrolled internal state
  const [uWidth, setUWidth] = useState<number>(width ?? defaultWidth);
  const [uHeight, setUHeight] = useState<number>(height ?? defaultHeight);

  // Resolved values and setters
  const widthValue = isControlled ? width : uWidth;
  const heightValue = isControlled ? height : uHeight;
  const setWidthValue = (value: number) => {
    (setWidth ?? setUWidth)(value);
  };
  const setHeightValue = (value: number) => {
    (setHeight ?? setUHeight)(value);
  };

  const [linked, setLinked] = useState(initialLinked);
  const [presetId, setPresetId] = useState<string | null>(null);
  const pendingApplyRef = useRef(false);

  // Keep a stable ratio while linked, so consecutive edits don't drift.
  const lockedRatioRef = useRef<AspectRatioParts | undefined>(undefined);

  const debounceTimeoutRef = useRef<number | null>(null);
  const pendingAdjustRef = useRef<{
    editingDimension: "height" | "width";
    ratio: AspectRatioParts | undefined;
    value: number;
  } | null>(null);

  useEffect(() => {
    if (linked && widthValue > 0 && heightValue > 0) {
      lockedRatioRef.current = reduceToRatio(widthValue, heightValue);
    } else {
      lockedRatioRef.current = undefined;
    }
  }, [linked, widthValue, heightValue]);

  useEffect(
    () => () => {
      if (debounceTimeoutRef.current != null) {
        window.clearTimeout(debounceTimeoutRef.current);
        debounceTimeoutRef.current = null;
      }
      pendingAdjustRef.current = null;
    },
    [],
  );

  const getLockedRatio = (): AspectRatioParts | undefined =>
    lockedRatioRef.current ??
    (widthValue > 0 && heightValue > 0
      ? reduceToRatio(widthValue, heightValue)
      : undefined);

  const adjustToRatio = (
    value: number,
    editingDimension: "width" | "height",
    ratio: AspectRatioParts | undefined,
  ) => {
    // If no ratio provided, just apply the direct edit
    if (!ratio) {
      if (editingDimension === "width") setWidthValue(value);
      else setHeightValue(value);
      return;
    }

    const { ratioHeight, ratioWidth } = ratio;

    if (ratioWidth <= 0 || ratioHeight <= 0) {
      if (editingDimension === "width") setWidthValue(value);
      else setHeightValue(value);
      return;
    }

    const divisor = editingDimension === "width" ? ratioWidth : ratioHeight;
    const multiplier = Math.max(1, Math.round(value / divisor));

    const newWidth = multiplier * ratioWidth;
    const newHeight = multiplier * ratioHeight;

    setWidthValue(newWidth);
    setHeightValue(newHeight);

    if (pendingApplyRef.current) {
      pendingApplyRef.current = false;
      // This doesn't happen faster than the state update
      // using width/height directly means out of date values
      onApply?.(newWidth, newHeight);
    }
  };

  const scheduleAdjustToRatio = (
    value: number,
    editingDimension: "width" | "height",
    ratio: AspectRatioParts | undefined,
  ) => {
    if (debounceTimeoutRef.current != null)
      window.clearTimeout(debounceTimeoutRef.current);

    pendingAdjustRef.current = { editingDimension, ratio, value };

    debounceTimeoutRef.current = window.setTimeout(() => {
      debounceTimeoutRef.current = null;
      const pending = pendingAdjustRef.current;
      pendingAdjustRef.current = null;
      if (pending) {
        adjustToRatio(pending.value, pending.editingDimension, pending.ratio);
      }
    }, DEBOUNCE_MS);
  };

  const applyPresetRatio = (ratio: AspectRatioParts) => {
    const { ratioHeight, ratioWidth } = ratio;

    const widthMultiplier = Math.max(1, Math.round(widthValue / ratioWidth));
    const w1 = widthMultiplier * ratioWidth;
    const h1 = widthMultiplier * ratioHeight;

    const heightMultiplier = Math.max(1, Math.round(heightValue / ratioHeight));
    const w2 = heightMultiplier * ratioWidth;
    const h2 = heightMultiplier * ratioHeight;

    const delta1 = Math.abs(w1 - widthValue) + Math.abs(h1 - heightValue);
    const delta2 = Math.abs(w2 - widthValue) + Math.abs(h2 - heightValue);

    if (delta1 <= delta2) {
      setWidthValue(w1);
      setHeightValue(h1);
    } else {
      setWidthValue(w2);
      setHeightValue(h2);
    }
  };

  const onChangeWidth = (value: number) => {
    setWidthValue(value);
    const presetRatio = parseRatioFromId(presetId);
    if (presetRatio) {
      scheduleAdjustToRatio(value, "width", presetRatio);
      return;
    }
    if (!linked) return;
    scheduleAdjustToRatio(value, "width", getLockedRatio());
  };

  const onChangeHeight = (value: number) => {
    setHeightValue(value);
    const presetRatio = parseRatioFromId(presetId);
    if (presetRatio) {
      scheduleAdjustToRatio(value, "height", presetRatio);
      return;
    }
    if (!linked) return;
    scheduleAdjustToRatio(value, "height", getLockedRatio());
  };

  const onPresetSelectionChange = (keys: Selection) => {
    if (debounceTimeoutRef.current != null) {
      window.clearTimeout(debounceTimeoutRef.current);
      debounceTimeoutRef.current = null;
    }
    pendingAdjustRef.current = null;

    let id: string | null = null;
    if (keys !== "all") {
      const first = [...keys][0] as string | undefined;
      id = first ?? null;
    }
    setPresetId(id);

    const ratio = parseRatioFromId(id);
    if (ratio) applyPresetRatio(ratio);
  };

  const onPressApply = () => {
    if (pendingAdjustRef.current) {
      pendingApplyRef.current = true;
    } else {
      onApply?.(widthValue, heightValue);
    }
  };

  const onPressPlatform = (
    width: number,
    height: number,
    aspectRatio: string,
  ) => {
    setWidthValue(width);
    setHeightValue(height);
    setPresetId(aspectRatio);

    onApply?.(width, height);
  };

  // Notify parent of the active aspect ratio to enforce in external resizers (e.g. RND)
  const lastRatioSentRef = useRef<number | undefined>(undefined);
  const onRatioChangeRef = useRef(onRatioChange);

  useEffect(() => {
    onRatioChangeRef.current = onRatioChange;
  }, [onRatioChange]);

  useEffect(() => {
    let ratioNum: number | undefined = undefined;
    const preset = parseRatioFromId(presetId);
    if (preset) {
      ratioNum = preset.ratioWidth / preset.ratioHeight;
    } else if (linked) {
      const r =
        lockedRatioRef.current ??
        (widthValue > 0 && heightValue > 0
          ? reduceToRatio(widthValue, heightValue)
          : undefined);
      if (r && r.ratioHeight > 0) {
        ratioNum = r.ratioWidth / r.ratioHeight;
      }
    }

    if (onRatioChangeRef.current && lastRatioSentRef.current !== ratioNum) {
      onRatioChangeRef.current(ratioNum);
      lastRatioSentRef.current = ratioNum;
    } else {
      lastRatioSentRef.current = ratioNum;
    }
  }, [presetId, linked, widthValue, heightValue]);

  return (
    <div className="flex flex-row gap-1.5 items-center">
      <PlatformIcons
        onPressInstagram={() => {
          onPressPlatform(1080, 1350, "4:5");
        }}
        onPressTiktok={() => {
          onPressPlatform(1080, 1920, "9:16");
        }}
        onPressYoutube={() => {
          onPressPlatform(1920, 1080, "16:9");
        }}
      />

      <div className="flex flex-row items-center">
        <NumberField
          {...numberFieldStyles}
          aria-label="Aspect Ratio Width"
          onChange={onChangeWidth}
          value={widthValue}
        />

        <ToggleButton
          isSelected={linked}
          off={<Unlink size={14} />}
          onChange={(isSelected) => {
            setLinked(isSelected);
          }}
          variant="ghost"
        >
          <Link size={14} />
        </ToggleButton>

        <NumberField
          {...numberFieldStyles}
          aria-label="Aspect Ratio Height"
          onChange={onChangeHeight}
          value={heightValue}
        />
      </div>

      <ToggleButtonGroup
        className="flex flex-row gap-1"
        onSelectionChange={onPresetSelectionChange}
        selectedKeys={(presetId ? new Set([presetId]) : new Set()) as Selection}
        selectionMode="single"
      >
        <ToggleButton id="16:9" size="sm">
          16:9
        </ToggleButton>
        <ToggleButton id="4:5" size="sm">
          4:5
        </ToggleButton>
        <ToggleButton id="9:16" size="sm">
          9:16
        </ToggleButton>
      </ToggleButtonGroup>

      {onApply && (
        <CheckOnClickButton
          blur="xs"
          onPress={onPressApply}
          showFocus={false}
          size="sm"
          variant="ghost"
        >
          <WandSparkles size={16} />
        </CheckOnClickButton>
      )}
    </div>
  );
};
