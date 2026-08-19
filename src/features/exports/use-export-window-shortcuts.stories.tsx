// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useMemo, useRef, useState } from "react";

import { Button } from "../../components/base/button/button";

import { createPlayhead } from "./components/scrub-playhead";
import { TimelineRuler } from "./components/scrub-timeline";
import { PREVIEW_FRAME_MS } from "./duration";
import { useExportWindowShortcuts } from "./use-export-window-shortcuts";

import type { Meta, StoryObj } from "@storybook/react-vite";

function ShortcutPreview() {
  const [activations, setActivations] = useState(0);
  const [isCropping, setIsCropping] = useState(false);
  const [isResizingCanvas, setIsResizingCanvas] = useState(false);
  const [isPlaying, setIsPlaying] = useState(false);
  const [lastAction, setLastAction] = useState("None");
  const [isSelecting, setIsSelecting] = useState(false);
  const [nudge, setNudge] = useState({ x: 0, y: 0 });
  const playhead = useMemo(() => createPlayhead(), []);
  // The arrows either nudge the selected layer or step the playhead, so the
  // story mirrors the editor: V picks the selection tool and swaps the two.
  const ratioRef = useRef(0);
  useExportWindowShortcuts({
    onCopy: () => {
      setLastAction("Copied");
    },
    onExport: () => {
      setLastAction("Exported");
    },
    onNudge: isSelecting
      ? (directionX, directionY, coarse) => {
          const step = coarse ? 10 : 1;
          setNudge((current) => ({
            x: current.x + directionX * step,
            y: current.y + directionY * step,
          }));
        }
      : undefined,
    onResizeCanvas: () => {
      setIsResizingCanvas((resizing) => !resizing);
    },
    onSelectTool: () => {
      setIsSelecting((selecting) => !selecting);
    },
    onStep: (direction, coarse) => {
      const stepMs = coarse ? 1_000 : PREVIEW_FRAME_MS;
      ratioRef.current = Math.min(
        1,
        Math.max(0, ratioRef.current + (direction * stepMs) / 5_000),
      );
      playhead.publish(ratioRef.current * 5, ratioRef.current);
    },
    onToggleCrop: () => {
      setIsCropping((cropping) => !cropping);
    },
    onTogglePlayback: () => {
      setIsPlaying((playing) => !playing);
    },
  });

  return (
    <div className="flex flex-col gap-4 text-content-fg">
      <div
        aria-label="Preview surface"
        className="flex h-48 w-96 items-center justify-center rounded-md bg-content/75 outline-none"
        tabIndex={0}
      >
        <span role="status">
          {isPlaying ? "Playing" : "Paused"} ·{" "}
          {isCropping ? "Crop on" : "Crop off"} · {lastAction} · Activations{" "}
          {activations} ·{" "}
          {isResizingCanvas ? "Canvas resize on" : "Canvas resize off"} ·{" "}
          {isSelecting ? "Select tool on" : "Select tool off"} · Nudge {nudge.x}
          , {nudge.y}
        </span>
      </div>
      <Button
        onPress={() => {
          setActivations((count) => count + 1);
        }}
      >
        Focused action
      </Button>
      <TimelineRuler
        durationMs={5_000}
        onSeek={(ratio) => {
          ratioRef.current = ratio;
          playhead.publish(ratio * 5, ratio);
        }}
        playhead={playhead}
      />
      <input
        aria-label="File name"
        className="rounded border border-muted/30 bg-content px-2 py-1 text-sm outline-none"
        defaultValue="Screenwide"
      />
    </div>
  );
}

const meta = {
  component: ShortcutPreview,
  parameters: { layout: "centered" },
  title: "Features/Window Shortcuts",
} satisfies Meta<typeof ShortcutPreview>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
