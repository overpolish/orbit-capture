// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Trash2, X } from "lucide-react";

import { ConfirmActionButton } from "./confirm-action-button";

import type { Meta, StoryObj } from "@storybook/react-vite";

const meta = {
  component: ConfirmActionButton,
  parameters: { layout: "centered" },
  title: "Confirm Action Button",
} satisfies Meta<typeof ConfirmActionButton>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    armedIcon: <Trash2 className="text-error" size={18} />,
    armedLabel: "Confirm delete",
    idleIcon: <X size={18} />,
    idleLabel: "Delete",
  },
};
