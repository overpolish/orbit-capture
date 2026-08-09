// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  getCurrentWindow,
  LogicalPosition,
  LogicalSize,
} from "@tauri-apps/api/window";
import { ReactNode, useEffect, useMemo, useRef } from "react";

import { ListBoxItem } from "../../components/base/listbox-item/listbox-item";
import { Select } from "../../components/base/select/select";

import { hideStandaloneListbox, showStandaloneListbox } from "./api";
import { initialStandaloneListboxHeight } from "./layout";
import { StandaloneListboxItem, useStandaloneListboxStore } from "./store";

type StandaloneMultiSelectProps = {
  exclusiveId: string;
  id: string;
  items: StandaloneListboxItem[];
  label: string;
  onSelectionChange: (items: StandaloneListboxItem[]) => void;
  placeholder: string;
  selectedIds: string[];
  leftSection?: ReactNode;
  onOpen?: () => Promise<StandaloneListboxItem[]>;
};

export function StandaloneMultiSelect({
  exclusiveId,
  id,
  items,
  label,
  leftSection,
  onOpen,
  onSelectionChange,
  placeholder,
  selectedIds,
}: StandaloneMultiSelectProps) {
  const active = useStandaloneListboxStore((state) => state.active);
  const close = useStandaloneListboxStore((state) => state.close);
  const lastSelection = useStandaloneListboxStore(
    (state) => state.lastSelection,
  );
  const open = useStandaloneListboxStore((state) => state.open);
  const handledEventRef = useRef(lastSelection?.eventId ?? null);
  const triggerRef = useRef<HTMLButtonElement>(null);

  const selectedItems = useMemo(
    () => items.filter((item) => selectedIds.includes(item.id)),
    [items, selectedIds],
  );
  const triggerItem = useMemo<StandaloneListboxItem | null>(() => {
    if (selectedItems.length === 0) return null;
    if (selectedItems.length === 1) return selectedItems[0];
    return {
      id: selectedItems.map((item) => item.id).join(","),
      label: `${selectedItems.length.toString()} applications`,
    };
  }, [selectedItems]);

  useEffect(() => {
    if (
      !lastSelection ||
      lastSelection.id !== id ||
      lastSelection.eventId === handledEventRef.current
    ) {
      return;
    }

    handledEventRef.current = lastSelection.eventId;
    onSelectionChange(
      items.filter((item) => lastSelection.selectedIds.includes(item.id)),
    );
  }, [id, items, lastSelection, onSelectionChange]);

  const showListbox = async () => {
    const trigger = triggerRef.current;
    if (!trigger) return;

    const bounds = trigger.getBoundingClientRect();
    const currentItems = onOpen ? await onOpen() : items;
    const height = initialStandaloneListboxHeight(currentItems.length);

    open({
      exclusiveId,
      id,
      items: currentItems,
      label,
      selectedIds,
      selectionMode: "multiple",
    });
    await showStandaloneListbox(
      getCurrentWindow().label,
      new LogicalPosition(bounds.left, bounds.bottom + 4),
      new LogicalSize(bounds.width, height),
    );
  };

  const toggleListbox = async () => {
    const isOpen = useStandaloneListboxStore.getState().active?.id === id;
    if (isOpen) {
      close();
      await hideStandaloneListbox();
    } else {
      await showListbox();
    }
  };

  return (
    <div
      className="w-full"
      onPointerDown={(event) => {
        event.stopPropagation();
      }}
    >
      <Select
        aria-label={label}
        className="w-full"
        clearable={false}
        isOpen={active?.id === id}
        items={triggerItem ? [triggerItem] : []}
        leftSection={leftSection}
        onPress={() => {
          void toggleListbox();
        }}
        placeholder={placeholder}
        showFocus={false}
        size="sm"
        standalone
        triggerRef={triggerRef}
        value={triggerItem?.id ?? null}
        variant="ghost"
      >
        {(item: StandaloneListboxItem) => (
          <ListBoxItem id={item.id} textValue={item.label}>
            {item.label}
          </ListBoxItem>
        )}
      </Select>
    </div>
  );
}
