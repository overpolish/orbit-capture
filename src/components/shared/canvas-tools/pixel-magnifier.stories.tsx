// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useState } from "react";

import { AnimatedPixelMagnifier } from "./pixel-magnifier";

import type { Meta, StoryObj } from "@storybook/react-vite";

const meta = {
  component: AnimatedPixelMagnifier,
  parameters: { layout: "centered" },
  title: "Pixel Magnifier",
} satisfies Meta<typeof AnimatedPixelMagnifier>;

export default meta;
type Story = StoryObj<typeof meta>;

function MagnifierExample() {
  const [source] = useState(() => {
    const canvas = document.createElement("canvas");
    canvas.height = 120;
    canvas.width = 120;
    const context = canvas.getContext("2d");
    if (!context) return null;

    context.fillStyle = "#171717";
    context.fillRect(0, 0, 120, 120);
    context.fillStyle = "#0ea5e9";
    context.fillRect(40, 40, 40, 40);
    context.fillStyle = "#fafafa";
    context.fillRect(54, 54, 12, 12);
    return canvas;
  });

  return (
    <div className="relative size-40 bg-neutral-950">
      <AnimatedPixelMagnifier
        className="absolute left-20 top-20 size-24"
        direction="bottomRight"
        point={{ x: 60, y: 60 }}
        source={source}
      />
    </div>
  );
}

export const Default: Story = {
  args: {
    direction: "bottomRight",
    point: { x: 60, y: 60 },
    source: null,
  },
  render: () => <MagnifierExample />,
};
