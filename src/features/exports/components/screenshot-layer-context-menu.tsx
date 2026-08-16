// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ChevronDown, ChevronUp, Trash2 } from "lucide-react";
import { ReactNode, useEffect } from "react";

import { Keyboard } from "../../../components/base/keyboard/keyboard";

export type LayerContextMenuState<ItemId = number> = {
  itemId: ItemId;
  x: number;
  y: number;
};

export type ScreenshotLayerContextMenuState = LayerContextMenuState;

export function LayerContextMenu<ItemId>({
  ariaLabel = "Layer actions",
  canDelete,
  menu,
  onClose,
  onDelete,
  onMoveBackward,
  onMoveForward,
  showDelete = true,
}: {
  canDelete: boolean;
  menu: LayerContextMenuState<ItemId>;
  onClose: () => void;
  onDelete: () => void;
  onMoveBackward: () => void;
  onMoveForward: () => void;
  ariaLabel?: string;
  showDelete?: boolean;
}) {
  useEffect(() => {
    window.addEventListener("blur", onClose);
    window.addEventListener("pointerdown", onClose);
    return () => {
      window.removeEventListener("blur", onClose);
      window.removeEventListener("pointerdown", onClose);
    };
  }, [onClose]);

  return (
    <div
      aria-label={ariaLabel}
      className="fixed z-50 w-48 overflow-hidden rounded-md border border-muted/25 bg-content p-1 shadow-lg"
      onContextMenu={(event) => {
        event.preventDefault();
      }}
      onPointerDown={(event) => {
        event.stopPropagation();
      }}
      role="menu"
      style={{ left: menu.x, top: menu.y }}
    >
      <Item
        icon={<ChevronUp size={14} />}
        label="Bring forwards"
        onPress={onMoveForward}
        shortcut="["
      />
      <Item
        icon={<ChevronDown size={14} />}
        label="Send backwards"
        onPress={onMoveBackward}
        shortcut="]"
      />
      {showDelete ? (
        <>
          <div className="my-1 h-px bg-muted/15" />
          <Item
            disabled={!canDelete}
            icon={<Trash2 size={14} />}
            label="Delete"
            onPress={onDelete}
            shortcut="⌫"
            tone="danger"
          />
        </>
      ) : null}
    </div>
  );
}

export function ScreenshotLayerContextMenu(
  props: Parameters<typeof LayerContextMenu<number>>[0],
) {
  return <LayerContextMenu {...props} ariaLabel="Screenshot layer actions" />;
}

function Item({
  disabled = false,
  icon,
  label,
  onPress,
  shortcut,
  tone = "default",
}: {
  icon: ReactNode;
  label: string;
  onPress: () => void;
  shortcut: string;
  disabled?: boolean;
  tone?: "danger" | "default";
}) {
  return (
    <button
      className={`flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm outline-none transition-colors ${
        tone === "danger"
          ? "text-danger enabled:hover:bg-danger/10"
          : "text-content-fg enabled:hover:bg-muted/10"
      } disabled:cursor-not-allowed disabled:opacity-40`}
      disabled={disabled}
      onClick={onPress}
      role="menuitem"
      type="button"
    >
      {icon}
      <span className="grow">{label}</span>
      <Keyboard size="xs">{shortcut}</Keyboard>
    </button>
  );
}
