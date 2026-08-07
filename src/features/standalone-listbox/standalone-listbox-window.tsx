import { convertFileSrc } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { AppWindowMac, Volume2 } from "lucide-react";
import { useLayoutEffect, useRef } from "react";

import { ListBox } from "../../components/base/listbox/listbox";
import { ListBoxItem } from "../../components/base/listbox-item/listbox-item";
import { OverflowShadow } from "../../components/base/overflow-shadow/overflow-shadow";

import { hideStandaloneListbox } from "./api";
import {
  emptyStandaloneListboxHeight,
  standaloneListboxMaxHeight,
} from "./layout";
import { useStandaloneListboxStore } from "./store";

export function StandaloneListboxWindow() {
  const active = useStandaloneListboxStore((state) => state.active);
  const close = useStandaloneListboxStore((state) => state.close);
  const select = useStandaloneListboxStore((state) => state.select);
  const listboxRef = useRef<HTMLDivElement>(null);
  const selectingRef = useRef(false);

  useLayoutEffect(() => {
    if (!active || !listboxRef.current) return;

    const window = getCurrentWindow();
    const resize = async () => {
      const scaleFactor = await window.scaleFactor();
      const currentSize = (await window.innerSize()).toLogical(scaleFactor);
      const height =
        active.items.length === 0
          ? emptyStandaloneListboxHeight
          : Math.min(
              listboxRef.current?.scrollHeight ?? currentSize.height,
              standaloneListboxMaxHeight,
            );
      await window.setSize(new LogicalSize(currentSize.width, height));
    };

    void resize();
  }, [active]);

  if (!active) return null;

  if (active.items.length === 0) {
    return (
      <div
        className="window-surface flex h-full min-h-16 w-full items-center justify-center rounded-[10px] px-3 text-center text-xs text-muted"
        ref={listboxRef}
      >
        No options available
      </div>
    );
  }

  const selectItem = (selectedId: number | string) => {
    if (selectingRef.current) return;

    selectingRef.current = true;
    select(active.id, [selectedId.toString()]);
    close();
    void hideStandaloneListbox().finally(() => {
      selectingRef.current = false;
    });
  };

  const onSelectionChange = (selection: "all" | Set<number | string>) => {
    if (selection === "all") return;
    if (active.selectionMode === "single") {
      const selected = selection.values().next();
      if (!selected.done) selectItem(selected.value);
      return;
    }

    const selectedIds = new Set(
      [...selection].map((selectedId) => selectedId.toString()),
    );
    const exclusiveId = active.exclusiveId;
    const previouslyExclusive = exclusiveId
      ? active.selectedIds.includes(exclusiveId)
      : false;
    if (exclusiveId && selectedIds.has(exclusiveId) && !previouslyExclusive) {
      select(active.id, [exclusiveId]);
      return;
    }
    if (exclusiveId) selectedIds.delete(exclusiveId);
    if (selectedIds.size === 0 && exclusiveId) selectedIds.add(exclusiveId);
    select(
      active.id,
      active.items
        .map((item) => item.id)
        .filter((itemId) => selectedIds.has(itemId)),
    );
  };

  return (
    <OverflowShadow rootClassName="window-surface" shadowRadius="md">
      <ListBox
        aria-label={active.label}
        className="window-surface w-full overflow-visible rounded-[10px]"
        onSelectionChange={onSelectionChange}
        ref={listboxRef}
        selectedKeys={active.selectedIds}
        selectionBehavior={
          active.selectionMode === "multiple" ? "toggle" : "replace"
        }
        selectionMode={active.selectionMode}
      >
        {active.items.map((item) => (
          <ListBoxItem
            className="min-h-7"
            compact
            id={item.id}
            key={item.id}
            onPress={() => {
              if (active.selectionMode === "single") selectItem(item.id);
            }}
            size="sm"
            textValue={item.label}
          >
            <span className="flex min-w-0 items-center gap-2">
              {item.iconPath ? (
                <img
                  alt=""
                  className="size-4 shrink-0 object-contain"
                  src={convertFileSrc(item.iconPath)}
                />
              ) : item.id === active.exclusiveId ? (
                <Volume2 className="shrink-0 text-muted" size={14} />
              ) : active.selectionMode === "multiple" ? (
                <AppWindowMac className="shrink-0 text-muted" size={14} />
              ) : null}
              <span className="truncate">{item.label}</span>
            </span>
          </ListBoxItem>
        ))}
      </ListBox>
    </OverflowShadow>
  );
}
