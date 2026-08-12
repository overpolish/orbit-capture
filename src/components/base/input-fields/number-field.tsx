// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { invoke, isTauri } from "@tauri-apps/api/core";
import { Minus, Plus } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  Button,
  Group,
  Input,
  NumberField as AriaNumberField,
  NumberFieldProps as AriaNumberFieldProps,
  Label,
} from "react-aria-components";
import { VariantProps } from "tailwind-variants";

import { tv } from "../../../lib/variants";
import { inputFieldVariants } from "../input-fields/input-field";

const ICON_SIZES = {
  md: 16,
  sm: 14,
};

const isMac = navigator.userAgent.includes("Mac");

const numberFieldVariants = tv({
  extend: inputFieldVariants,
  slots: {
    stepper: [
      "text-muted/75 px-3 self-stretch transition-colors",
      "data-[hovered]:text-content-fg",
    ],
  },
});

export type NumberFieldProps = AriaNumberFieldProps &
  VariantProps<typeof numberFieldVariants> & {
    centered?: boolean;
    className?: string;
    fieldClassName?: string;
    label?: string;
    labelClassName?: string;
    leftSection?: React.ReactNode;
    rightAligned?: boolean;
    rightSection?: React.ReactNode;
    scrubStep?: number;
    scrubbable?: boolean;
    showSteppers?: boolean;
    size?: "sm" | "md";
    variant?: "ghost" | "line" | "solid";
  };

export const NumberField = ({
  centered,
  className,
  defaultValue,
  fieldClassName,
  label,
  labelClassName,
  leftSection,
  maxValue,
  minValue,
  onChange,
  rightAligned,
  rightSection,
  scrubStep,
  scrubbable = false,
  showSteppers = true,
  size,
  step,
  value,
  variant,
  ...props
}: NumberFieldProps) => {
  const [uncontrolledValue, setUncontrolledValue] = useState(defaultValue ?? 0);
  const resolvedValue = value ?? uncontrolledValue;
  const valueRef = useRef(resolvedValue);
  const groupRef = useRef<HTMLDivElement>(null);
  const cursorScrubRef = useRef<Promise<unknown> | null>(null);
  const dragRef = useRef<{
    residual: number;
    scrubbing: boolean;
    startTravel: number;
  } | null>(null);

  valueRef.current = resolvedValue;

  const changeValue = useCallback(
    (nextValue: number) => {
      const clampedValue = Math.min(
        maxValue ?? Number.POSITIVE_INFINITY,
        Math.max(minValue ?? Number.NEGATIVE_INFINITY, nextValue),
      );
      const precision = Math.max(
        0,
        String(scrubStep ?? step ?? 1).split(".")[1]?.length ?? 0,
      );
      const roundedValue = Number(clampedValue.toFixed(precision));

      valueRef.current = roundedValue;
      if (value === undefined) {
        setUncontrolledValue(roundedValue);
      }
      onChange?.(roundedValue);
    },
    [maxValue, minValue, onChange, scrubStep, step, value],
  );

  const releaseCursorScrub = useCallback(() => {
    document.documentElement.removeAttribute("data-number-field-scrubbing");
    if (cursorScrubRef.current) {
      const cursorScrub = cursorScrubRef.current;
      cursorScrubRef.current = null;
      void cursorScrub
        .catch(() => undefined)
        .then(() => invoke("end_cursor_scrub"))
        .catch(() => undefined);
    } else if (document.pointerLockElement) {
      document.exitPointerLock();
    }
  }, []);

  useEffect(
    () => () => {
      releaseCursorScrub();
    },
    [releaseCursorScrub],
  );

  useEffect(() => {
    if (!scrubbable) {
      return;
    }

    const handleMouseMove = (event: MouseEvent) => {
      const drag = dragRef.current;
      if (!drag) {
        return;
      }

      const travel = event.movementX - event.movementY;
      if (!drag.scrubbing) {
        drag.startTravel += travel;
        if (Math.abs(drag.startTravel) < 3) {
          return;
        }
        drag.scrubbing = true;
      }

      drag.residual += travel;
      const stepsMoved = Math.trunc(drag.residual / 4);
      if (stepsMoved === 0) {
        return;
      }

      drag.residual -= stepsMoved * 4;
      const multiplier = (event.shiftKey ? 10 : 1) * (event.altKey ? 0.1 : 1);
      changeValue(
        valueRef.current + stepsMoved * (scrubStep ?? step ?? 1) * multiplier,
      );
    };

    const handleMouseUp = () => {
      const drag = dragRef.current;
      if (!drag) {
        return;
      }

      dragRef.current = null;
      releaseCursorScrub();

      if (!drag.scrubbing) {
        const input = groupRef.current?.querySelector("input");
        input?.focus();
        input?.select();
      }
    };

    document.addEventListener("mousemove", handleMouseMove);
    document.addEventListener("mouseup", handleMouseUp);
    window.addEventListener("blur", handleMouseUp);
    return () => {
      document.removeEventListener("mousemove", handleMouseMove);
      document.removeEventListener("mouseup", handleMouseUp);
      window.removeEventListener("blur", handleMouseUp);
    };
  }, [changeValue, releaseCursorScrub, scrubbable, scrubStep, step]);

  const handlePointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (!scrubbable || event.button !== 0) {
      return;
    }

    dragRef.current = { residual: 0, scrubbing: false, startTravel: 0 };
    if (isTauri() && isMac) {
      document.documentElement.setAttribute("data-number-field-scrubbing", "");
      cursorScrubRef.current = invoke("begin_cursor_scrub");
    } else if (isTauri()) {
      void groupRef.current?.requestPointerLock().catch(() => undefined);
    }
  };

  const {
    base,
    field,
    input,
    inputWrapper,
    label: _label,
    line,
    stepper,
  } = numberFieldVariants({ centered, rightAligned, size, variant });

  return (
    <AriaNumberField
      {...props}
      className={base({ className })}
      maxValue={maxValue}
      minValue={minValue}
      onChange={changeValue}
      step={step}
      value={resolvedValue}
    >
      {label && (
        <Label className={_label({ className: labelClassName })}>{label}</Label>
      )}

      <Group
        className={field({
          className: [
            scrubbable ? "cursor-ew-resize" : undefined,
            fieldClassName,
          ],
        })}
        onPointerDown={handlePointerDown}
        ref={groupRef}
      >
        {showSteppers && (
          <Button aria-label="Decrement" className={stepper()} slot="decrement">
            <Minus size={size ? ICON_SIZES[size] : 16} />
          </Button>
        )}

        <div className={inputWrapper()}>
          {leftSection}

          <Input className={input()} />

          {rightSection}
        </div>

        {showSteppers && (
          <Button aria-label="Increment" className={stepper()} slot="increment">
            <Plus size={size ? ICON_SIZES[size] : 16} />
          </Button>
        )}

        {variant === "line" && <div className={line()} />}
      </Group>
    </AriaNumberField>
  );
};
