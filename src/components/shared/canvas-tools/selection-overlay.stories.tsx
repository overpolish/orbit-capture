// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { SelectionOverlay } from "./selection-overlay";

import type { Meta, StoryObj } from "@storybook/react-vite";

const meta = {
  component: SelectionOverlay,
  decorators: [
    (Story) => (
      <div className="relative h-36 w-80 bg-neutral-950">
        <Story />
      </div>
    ),
  ],
  parameters: { layout: "centered" },
  title: "Selection Overlay",
} satisfies Meta<typeof SelectionOverlay>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    regions: [
      { height: 0.18, width: 0.76, x: 0.08, y: 0.2 },
      { height: 0.18, width: 0.58, x: 0.08, y: 0.52 },
    ],
    selectedRegions: [{ height: 0.18, width: 0.32, x: 0.42, y: 0.2 }],
  },
};
