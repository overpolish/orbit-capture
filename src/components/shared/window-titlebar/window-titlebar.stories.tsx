// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Button } from "../../base/button/button";

import { WindowTitlebar } from "./window-titlebar";

import type { Meta, StoryObj } from "@storybook/react-vite";

const meta = {
  component: WindowTitlebar,
  decorators: [
    (Story) => (
      <div className="w-[560px] bg-content text-content-fg">
        <Story />
      </div>
    ),
  ],
  parameters: { layout: "centered" },
  title: "Window Titlebar",
} satisfies Meta<typeof WindowTitlebar>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    actions: (
      <Button showFocus={false} size="sm" variant="ghost">
        Save
      </Button>
    ),
    center: <span className="text-xs text-muted">General</span>,
    title: "Orbit Capture",
  },
};
