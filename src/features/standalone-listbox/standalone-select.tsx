import {
  getCurrentWindow,
  LogicalPosition,
  LogicalSize,
} from "@tauri-apps/api/window";
import { ReactNode, useEffect, useRef } from "react";

import { ListBoxItem } from "../../components/base/listbox-item/listbox-item";
import { Select } from "../../components/base/select/select";

import { hideStandaloneListbox, showStandaloneListbox } from "./api";
import { initialStandaloneListboxHeight } from "./layout";
import { StandaloneListboxItem, useStandaloneListboxStore } from "./store";

type StandaloneSelectProps = {
  id: string;
  items: StandaloneListboxItem[];
  label: string;
  onSelectionChange: (item: StandaloneListboxItem) => void;
  placeholder: string;
  selectedId: string | null;
  leftSection?: ReactNode;
  onOpen?: () => Promise<StandaloneListboxItem[]>;
};

export function StandaloneSelect({
  id,
  items,
  label,
  leftSection,
  onOpen,
  onSelectionChange,
  placeholder,
  selectedId,
}: StandaloneSelectProps) {
  const active = useStandaloneListboxStore((state) => state.active);
  const lastSelection = useStandaloneListboxStore(
    (state) => state.lastSelection,
  );
  const close = useStandaloneListboxStore((state) => state.close);
  const open = useStandaloneListboxStore((state) => state.open);
  const handledEventRef = useRef(lastSelection?.eventId ?? null);
  const triggerRef = useRef<HTMLButtonElement>(null);

  const selectedItem = items.find((item) => item.id === selectedId) ?? null;

  useEffect(() => {
    if (
      !lastSelection ||
      lastSelection.id !== id ||
      lastSelection.eventId === handledEventRef.current
    ) {
      return;
    }

    handledEventRef.current = lastSelection.eventId;
    const item = items.find(
      (candidate) => candidate.id === lastSelection.selectedIds[0],
    );
    if (item) onSelectionChange(item);
  }, [id, items, lastSelection, onSelectionChange]);

  const showListbox = async () => {
    const trigger = triggerRef.current;
    if (!trigger) return;

    const bounds = trigger.getBoundingClientRect();
    const currentItems = onOpen ? await onOpen() : items;
    const height = initialStandaloneListboxHeight(currentItems.length);

    open({
      id,
      items: currentItems,
      label,
      selectedIds: selectedId ? [selectedId] : [],
      selectionMode: "single",
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
        items={selectedItem ? [selectedItem] : []}
        leftSection={leftSection}
        onPress={() => {
          void toggleListbox();
        }}
        placeholder={placeholder}
        showFocus={false}
        size="sm"
        standalone
        triggerRef={triggerRef}
        value={selectedId}
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
