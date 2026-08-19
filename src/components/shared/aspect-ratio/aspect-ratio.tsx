// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Link, RotateCcw, Unlink, WandSparkles } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { cn } from "../../../lib/styling";
import { Button } from "../../base/button/button";
import { ToggleButton } from "../../base/button/toggle-button";
import { NumberField } from "../../base/input-fields/number-field";
import { CheckOnClickButton } from "../check-on-click-button/check-on-click-button";

import {
  AspectRatioParts,
  closestDimensionsAtRatio,
  dimensionsAtRatio,
  matchesRatio,
  parseRatioFromId,
  reduceToRatio,
} from "./aspect-ratio-math";
import { PlatformPresets } from "./platform-presets";

const numberFieldStyles: React.ComponentProps<typeof NumberField> = {
  centered: true,
  className: "w-15",
  scrubbable: true,
  showSteppers: false,
  size: "sm",
  variant: "line",
};

type AspectRatioProps = {
  className?: string;
  defaultHeight?: number;
  defaultWidth?: number;
  height?: number;
  initialLinked?: boolean;
  label?: string;
  layout?: "inline" | "stacked";
  onApply?: (width: number, height: number) => void;
  onRatioChange?: (ratio: number | undefined) => void;
  onReset?: () => void;
  setDimensions?: (width: number, height: number) => void;
  setHeight?: (value: number) => void;
  setWidth?: (value: number) => void;
  width?: number;
};

export const AspectRatio = ({
  className,
  defaultHeight = 1080,
  defaultWidth = 1920,
  height,
  initialLinked = false,
  label = "Dimensions",
  layout = "inline",
  onApply,
  onRatioChange,
  onReset,
  setDimensions,
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
  const requestedDimensionsRef = useRef<{
    height: number;
    width: number;
  } | null>(null);

  // Resolved values and setters
  const widthValue = isControlled ? width : uWidth;
  const heightValue = isControlled ? height : uHeight;
  const setWidthValue = (value: number) => {
    (setWidth ?? setUWidth)(value);
  };
  const setHeightValue = (value: number) => {
    (setHeight ?? setUHeight)(value);
  };
  const setDimensionValues = (newWidth: number, newHeight: number) => {
    if (setDimensions) {
      requestedDimensionsRef.current = {
        height: newHeight,
        width: newWidth,
      };
      setDimensions(newWidth, newHeight);
      return;
    }
    setWidthValue(newWidth);
    setHeightValue(newHeight);
  };

  const [linked, setLinked] = useState(initialLinked);

  // Keep a stable ratio while linked, so consecutive edits don't drift.
  const lockedRatioRef = useRef<AspectRatioParts | undefined>(
    initialLinked && widthValue > 0 && heightValue > 0
      ? reduceToRatio(widthValue, heightValue)
      : undefined,
  );
  const dimensionsRef = useRef({ height: heightValue, width: widthValue });
  dimensionsRef.current = { height: heightValue, width: widthValue };

  useLayoutEffect(() => {
    if (!linked || widthValue <= 0 || heightValue <= 0) return;

    const requested = requestedDimensionsRef.current;
    requestedDimensionsRef.current = null;
    if (
      requested &&
      requested.width === widthValue &&
      requested.height === heightValue
    )
      return;

    // Whole-pixel dimensions rarely land exactly on the locked ratio, so a
    // gesture that honours it would still redefine it a little every frame.
    // Dimensions already on the ratio leave it as it is.
    const locked = lockedRatioRef.current;
    if (locked && matchesRatio(widthValue, heightValue, locked)) return;

    // Controlled dimensions can begin with temporary placeholder values while
    // their artifact loads, or change through reset/undo. Treat those external
    // values as the ratio to preserve without letting our own linked edits
    // redefine it on every scrub step.
    lockedRatioRef.current = reduceToRatio(widthValue, heightValue);
  }, [heightValue, linked, widthValue]);

  useEffect(() => {
    if (!linked) {
      lockedRatioRef.current = undefined;
      return;
    }

    const dimensions = dimensionsRef.current;
    // Empty dimensions are not a ratio to keep: linking is often turned on by
    // a preset chosen before anything has been drawn, and the ratio that
    // preset just locked is the one to draw at.
    if (dimensions.width > 0 && dimensions.height > 0) {
      lockedRatioRef.current = reduceToRatio(
        dimensions.width,
        dimensions.height,
      );
    }
  }, [linked]);

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

    const { height: newHeight, width: newWidth } = dimensionsAtRatio(
      value,
      editingDimension,
      ratio,
    );

    setDimensionValues(newWidth, newHeight);
  };

  const applyPresetRatio = (ratio: AspectRatioParts) => {
    const dimensions = closestDimensionsAtRatio(widthValue, heightValue, ratio);
    lockedRatioRef.current = ratio;
    setLinked(true);
    setDimensionValues(dimensions.width, dimensions.height);
  };

  const onChangeWidth = (value: number) => {
    if (linked) {
      adjustToRatio(value, "width", getLockedRatio());
      return;
    }
    setWidthValue(value);
  };

  const onChangeHeight = (value: number) => {
    if (linked) {
      adjustToRatio(value, "height", getLockedRatio());
      return;
    }
    setHeightValue(value);
  };

  const onPressPreset = (id: string) => {
    const ratio = parseRatioFromId(id);
    if (ratio) applyPresetRatio(ratio);
  };

  const onPressApply = () => {
    onApply?.(widthValue, heightValue);
  };

  const onPressPlatform = (
    width: number,
    height: number,
    aspectRatio: string,
  ) => {
    const ratio = parseRatioFromId(aspectRatio);
    if (ratio) lockedRatioRef.current = ratio;
    setLinked(true);
    setDimensionValues(width, height);

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
    if (linked) {
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
  }, [linked, widthValue, heightValue]);

  const platformIcons = (
    <PlatformPresets
      onInstagram={() => {
        onPressPlatform(1080, 1350, "4:5");
      }}
      onTiktok={() => {
        onPressPlatform(1080, 1920, "9:16");
      }}
      onYoutube={() => {
        onPressPlatform(1920, 1080, "16:9");
      }}
    />
  );
  const dimensionFields = (
    <div className="flex flex-row items-center">
      {onReset ? (
        <Button
          aria-label="Reset dimensions"
          icon
          onPress={onReset}
          showFocus={false}
          size="sm"
          variant="ghost"
        >
          <RotateCcw size={13} />
        </Button>
      ) : null}
      <NumberField
        {...numberFieldStyles}
        aria-label="Aspect Ratio Width"
        onChange={onChangeWidth}
        value={widthValue}
      />

      <ToggleButton
        aria-label={linked ? "Unlink dimensions" : "Link dimensions"}
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
  );
  const ratioButtons = (
    <div className="flex flex-row gap-1">
      <Button
        className="border border-muted/30 text-muted"
        onPress={() => {
          onPressPreset("16:9");
        }}
        size="sm"
        variant="ghost"
      >
        16:9
      </Button>
      <Button
        className="border border-muted/30 text-muted"
        onPress={() => {
          onPressPreset("4:5");
        }}
        size="sm"
        variant="ghost"
      >
        4:5
      </Button>
      <Button
        className="border border-muted/30 text-muted"
        onPress={() => {
          onPressPreset("9:16");
        }}
        size="sm"
        variant="ghost"
      >
        9:16
      </Button>
    </div>
  );

  return (
    <div
      className={cn(
        layout === "stacked"
          ? "flex w-full flex-col gap-2"
          : "flex flex-row items-center gap-1.5",
        className,
      )}
    >
      {layout === "stacked" ? (
        <>
          <div className="flex items-center">
            <span className="text-xs text-content-fg">{label}</span>
            <div className="grow" />
            {dimensionFields}
          </div>
          <div className="flex items-center">
            {platformIcons}
            <div className="grow" />
            {ratioButtons}
          </div>
        </>
      ) : (
        <>
          {platformIcons}
          {dimensionFields}
          {ratioButtons}
        </>
      )}

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
