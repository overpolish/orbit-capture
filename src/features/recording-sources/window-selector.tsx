// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { convertFileSrc } from "@tauri-apps/api/core";
import { AppWindowMac, CircleSlash2, LoaderCircle } from "lucide-react";

import { Button } from "../../components/base/button/button";
import { OverflowShadow } from "../../components/base/overflow-shadow/overflow-shadow";

import { WindowDetails } from "./types";

type WindowSelectorProps = {
  error: string | null;
  isLoading: boolean;
  onSelect: (window: WindowDetails) => void;
  selectedWindow: WindowDetails | null;
  windows: WindowDetails[];
};

export function WindowSelector({
  error,
  isLoading,
  onSelect,
  selectedWindow,
  windows,
}: WindowSelectorProps) {
  if (isLoading) {
    return (
      <div className="relative h-full">
        <LoaderCircle
          className="spinner-pixel-centered animate-spin text-muted"
          size={48}
        />
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex h-full items-center justify-center px-8 text-center text-xs text-danger">
        {error}
      </div>
    );
  }

  if (windows.length === 0) {
    return (
      <div className="flex h-full items-center justify-center gap-3 text-sm font-semibold text-muted">
        <CircleSlash2 size={36} />
        No windows found
      </div>
    );
  }

  return (
    <OverflowShadow
      className="grid grid-cols-4 gap-2 p-3"
      insetShadow
      orientation="vertical"
      shadowRadius="md"
    >
      {[...windows]
        .sort((left, right) => {
          const appOrder = left.appName.localeCompare(
            right.appName,
            undefined,
            {
              sensitivity: "base",
            },
          );
          if (appOrder !== 0) return appOrder;

          return left.title.localeCompare(right.title, undefined, {
            sensitivity: "base",
          });
        })
        .map((window) => {
          const isSelected = selectedWindow?.id === window.id;

          return (
            <Button
              aria-label={`Select ${window.appName}: ${window.title}`}
              className="relative min-h-30 min-w-0 flex-col items-stretch justify-start gap-2 p-2 ring-1 ring-inset ring-content-fg/5"
              color={isSelected ? "info" : "neutral"}
              key={window.id}
              onPress={() => {
                onSelect(window);
              }}
              showFocus={false}
              variant={isSelected ? "soft" : "ghost"}
            >
              <span className="sticky top-2 z-10 flex w-full min-w-0 items-center gap-1.5 rounded-sm bg-content/50 p-1 text-left backdrop-blur-xs">
                {window.appIconPath ? (
                  <img
                    alt=""
                    className="size-4 shrink-0 object-contain"
                    src={convertFileSrc(window.appIconPath)}
                  />
                ) : (
                  <AppWindowMac className="shrink-0 text-muted" size={16} />
                )}
                <span className="min-w-0 truncate text-xxs font-medium">
                  {window.title}
                </span>
              </span>

              <span className="flex min-h-0 grow items-center justify-center overflow-hidden">
                {window.thumbnailPath ? (
                  <img
                    alt=""
                    className="max-h-full max-w-full rounded-sm object-contain shadow-md"
                    src={convertFileSrc(window.thumbnailPath)}
                  />
                ) : (
                  <span className="flex flex-col items-center gap-1 text-[9px] text-muted">
                    <AppWindowMac size={24} />
                    Preview unavailable
                  </span>
                )}
              </span>
            </Button>
          );
        })}
    </OverflowShadow>
  );
}
