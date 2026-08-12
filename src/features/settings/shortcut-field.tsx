// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { X } from "lucide-react";
import { useEffect, useState } from "react";

import { Button } from "../../components/base/button/button";
import { Keyboard } from "../../components/base/keyboard/keyboard";

import { beginShortcutCapture, endShortcutCapture } from "./api";

const keyName = (code: string) => {
  if (code.startsWith("Key")) return code.slice(3);
  if (code.startsWith("Digit")) return code.slice(5);
  return code.replace("Arrow", "");
};

const shortcutFromEvent = (event: KeyboardEvent) => {
  if (["Alt", "Control", "Meta", "Shift"].includes(event.key)) return null;
  const modifiers = [
    event.metaKey ? "Command" : null,
    event.ctrlKey ? "Control" : null,
    event.altKey ? "Alt" : null,
    event.shiftKey ? "Shift" : null,
  ].filter(Boolean);
  if (modifiers.length === 0) return null;
  return [...modifiers, event.code].join("+");
};

const displayShortcut = (shortcut: string | null) => {
  if (!shortcut) return [];
  const mac = navigator.userAgent.includes("Mac");
  return shortcut.split("+").map((part) => {
    if (part === "CommandOrControl") return mac ? "⌘" : "Ctrl";
    if (part === "Command" || part === "Super") return mac ? "⌘" : "Win";
    if (part === "Control") return mac ? "⌃" : "Ctrl";
    if (part === "Alt") return mac ? "⌥" : "Alt";
    if (part === "Shift") return "⇧";
    return keyName(part);
  });
};

export function ShortcutField({
  isDisabled,
  onChange,
  value,
}: {
  onChange: (shortcut: string | null) => void;
  value: string | null;
  isDisabled?: boolean;
}) {
  const [listening, setListening] = useState(false);

  useEffect(() => {
    if (!listening) return;
    const onKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Escape") {
        setListening(false);
        void endShortcutCapture();
        return;
      }
      if (event.key === "Backspace" || event.key === "Delete") {
        onChange(null);
        setListening(false);
        return;
      }
      const shortcut = shortcutFromEvent(event);
      if (!shortcut) return;
      onChange(shortcut);
      setListening(false);
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
    };
  }, [listening, onChange]);

  const keys = displayShortcut(value);

  return (
    <div className="flex items-center gap-1">
      <Button
        className="min-w-32 justify-center whitespace-nowrap"
        isDisabled={isDisabled}
        onPress={() => {
          void beginShortcutCapture().then(() => {
            setListening(true);
          });
        }}
        showFocus={false}
        size="sm"
        variant="ghost"
      >
        {listening ? (
          <span className="text-xs text-muted">Press shortcut…</span>
        ) : keys.length > 0 ? (
          <span className="flex items-center gap-1">
            {keys.map((key, index) => (
              <Keyboard key={`${key}-${index.toString()}`} size="sm">
                {key}
              </Keyboard>
            ))}
          </span>
        ) : (
          <span className="text-xs text-muted">Not set</span>
        )}
      </Button>
      <Button
        aria-label="Clear shortcut"
        icon
        isDisabled={isDisabled || value === null}
        onPress={() => {
          onChange(null);
        }}
        showFocus={false}
        size="sm"
        variant="ghost"
      >
        <X size={14} />
      </Button>
    </div>
  );
}
