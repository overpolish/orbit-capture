// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useMemo, useState } from "react";

import { createPlayhead } from "./components/scrub-playhead";
import { TimelineRuler } from "./components/scrub-timeline";
import { useExportWindowShortcuts } from "./use-export-window-shortcuts";

import type { Meta, StoryObj } from "@storybook/react-vite";

function ShortcutPreview() {
  const [isPlaying, setIsPlaying] = useState(false);
  const playhead = useMemo(() => createPlayhead(), []);
  useExportWindowShortcuts({
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
        <span role="status">{isPlaying ? "Playing" : "Paused"}</span>
      </div>
      <TimelineRuler
        durationMs={5_000}
        onSeek={(ratio) => {
          playhead.publish(ratio * 5, ratio);
        }}
        playhead={playhead}
      />
      <input
        aria-label="File name"
        className="rounded border border-muted/30 bg-content px-2 py-1 text-sm outline-none"
        defaultValue="Orbit Capture"
      />
    </div>
  );
}

const meta = {
  component: ShortcutPreview,
  parameters: { layout: "centered" },
  title: "Export/Window Shortcuts",
} satisfies Meta<typeof ShortcutPreview>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
