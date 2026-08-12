// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { SelectionFrame } from "./selection-frame";
import { SelectionOverlay } from "./selection-overlay";

import type { Meta, StoryObj } from "@storybook/react-vite";

const meta = {
  component: SelectionFrame,
  decorators: [
    (Story) => (
      <div className="relative h-64 w-96 bg-neutral-950">
        <Story />
      </div>
    ),
  ],
  parameters: { layout: "centered" },
  title: "Selection Frame",
} satisfies Meta<typeof SelectionFrame>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    bounds: { height: 144, width: 280, x: 52, y: 56 },
    children: (
      <SelectionOverlay
        regions={[
          { height: 0.16, width: 0.72, x: 0.08, y: 0.18 },
          { height: 0.16, width: 0.82, x: 0.08, y: 0.48 },
        ]}
        selectedRegions={[{ height: 0.16, width: 0.34, x: 0.46, y: 0.18 }]}
      />
    ),
    state: "ready",
  },
};

export const Loading: Story = {
  args: {
    bounds: { height: 144, width: 280, x: 52, y: 56 },
    state: "loading",
  },
};
