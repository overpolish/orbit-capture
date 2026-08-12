// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useRef, useState } from "react";
import { Group, Input, Label, TextField } from "react-aria-components";

import { inputFieldVariants } from "./input-field";

const hexCharacters = /^[0-9a-f]+$/i;

const normalizeHexColor = (value: string): string | undefined => {
  const hex = value.trim().replace(/^#/, "");
  if (!hexCharacters.test(hex)) return undefined;

  const normalized =
    hex.length === 2
      ? hex.repeat(3)
      : hex.length === 3
        ? hex.replace(/(.)/g, "$1$1")
        : hex.length === 6
          ? hex
          : undefined;

  return normalized ? `#${normalized.toUpperCase()}` : undefined;
};

export function ColorField({
  isDisabled,
  label,
  onChange,
  value,
}: {
  label: string;
  value: string;
  isDisabled?: boolean;
  onChange?: (value: string) => void;
}) {
  const [draft, setDraft] = useState(value.replace(/^#/, "").toUpperCase());
  const lastValueRef = useRef(value);
  const styles = inputFieldVariants({ size: "sm", variant: "line" });

  if (value !== lastValueRef.current) {
    lastValueRef.current = value;
    if (normalizeHexColor(draft) !== value.toUpperCase()) {
      setDraft(value.replace(/^#/, "").toUpperCase());
    }
  }

  const emit = (nextValue: string) => {
    const normalized = normalizeHexColor(nextValue);
    if (!normalized) return;
    lastValueRef.current = normalized;
    onChange?.(normalized);
  };

  const pickerValue = normalizeHexColor(draft) ?? value;

  return (
    <TextField
      className="flex items-center justify-between gap-3"
      isDisabled={isDisabled}
      onChange={(nextValue) => {
        const nextDraft = nextValue.replace(/^#/, "").toUpperCase();
        setDraft(nextDraft);
        emit(nextDraft);
      }}
      value={draft}
    >
      <Label className="text-xs text-content-fg">{label}</Label>
      <Group className={styles.field({ className: "w-28 shrink-0" })}>
        <div className={styles.inputWrapper()}>
          <span aria-hidden className="shrink-0 text-xs text-muted">
            #
          </span>
          <Input
            aria-label={label}
            className={styles.input()}
            maxLength={6}
            onBlur={() => {
              if (!normalizeHexColor(draft)) {
                setDraft(value.replace(/^#/, "").toUpperCase());
              }
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") event.currentTarget.blur();
              if (event.key === "Escape") {
                setDraft(value.replace(/^#/, "").toUpperCase());
                event.currentTarget.blur();
              }
            }}
          />
          <span className="relative size-4 shrink-0 overflow-hidden rounded-sm border border-muted/30">
            <span
              aria-hidden
              className="pointer-events-none absolute inset-0"
              style={{ backgroundColor: pickerValue }}
            />
            <input
              aria-label={`${label} picker`}
              className="absolute inset-0 size-full cursor-pointer opacity-0"
              disabled={isDisabled}
              onChange={(event) => {
                const nextValue = event.currentTarget.value.toUpperCase();
                setDraft(nextValue.replace(/^#/, ""));
                emit(nextValue);
              }}
              type="color"
              value={pickerValue}
            />
          </span>
        </div>
        <div className={styles.line()} />
      </Group>
    </TextField>
  );
}
