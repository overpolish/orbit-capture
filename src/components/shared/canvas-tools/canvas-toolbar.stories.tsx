// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Copy, RotateCcw, X } from "lucide-react";

import { Button } from "../../base/button/button";

import { CanvasToolbar } from "./canvas-toolbar";

import type { Meta, StoryObj } from "@storybook/react-vite";

const meta = {
  component: CanvasToolbar,
  parameters: { layout: "centered" },
  title: "Canvas Toolbar",
} satisfies Meta<typeof CanvasToolbar>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    children: (
      <>
        <Button showFocus={false} size="sm" variant="ghost">
          <Copy size={15} />
          Copy
        </Button>
        <Button
          aria-label="Reset"
          icon
          showFocus={false}
          size="sm"
          variant="ghost"
        >
          <RotateCcw size={15} />
        </Button>
        <Button
          aria-label="Close"
          icon
          showFocus={false}
          size="sm"
          variant="ghost"
        >
          <X size={15} />
        </Button>
      </>
    ),
  },
};
