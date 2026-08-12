// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Meta, StoryObj } from "@storybook/react";

import { CheckOnClickButton } from "./check-on-click-button";

const meta = {
  args: {
    children: "Action",
  },
  component: CheckOnClickButton,
  parameters: {
    layout: "centered",
  },
  title: "Check On Click Button",
} satisfies Meta<typeof CheckOnClickButton>;

export default meta;
type Story = StoryObj<typeof meta>;

/* --------------------------------- Stories -------------------------------- */
export const Default: Story = {};
