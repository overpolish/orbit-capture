// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { TransformControls } from "./transform-controls";

import type { Meta, StoryObj } from "@storybook/react-vite";

const meta = {
  component: TransformControls,
  decorators: [
    (Story) => (
      <div className="relative h-64 w-96 bg-neutral-950">
        <Story />
      </div>
    ),
  ],
  parameters: { layout: "centered" },
  title: "On Screen Controls",
} satisfies Meta<typeof TransformControls>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    frame: { height: 140, width: 230, x: 82, y: 62 },
    move: { label: "Move selection", onPointerDown: () => undefined },
    radius: 18,
    radiusHandle: {
      cursor: "nwse-resize",
      label: "Corner radius",
      left: "26px",
      onPointerDown: () => undefined,
      top: "26px",
    },
    resize: {
      label: (edges) => `Resize ${edges.join(" ")}`,
      onPointerDown: () => () => undefined,
    },
    scaleRing: {
      cursor: "nesw-resize",
      extent: 48,
      label: "Scale selection",
      onPointerDown: () => undefined,
    },
  },
};
