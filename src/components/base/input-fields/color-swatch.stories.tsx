// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useState } from "react";

import { ColorSwatch } from "./color-swatch";

import type { Meta, StoryObj } from "@storybook/react-vite";

const meta = {
  component: ColorSwatch,
  parameters: { layout: "centered" },
  title: "Color Swatch",
} satisfies Meta<typeof ColorSwatch>;

export default meta;
type Story = StoryObj<typeof meta>;

function SwatchExample({ isDisabled = false, isLocked = false }) {
  const [value, setValue] = useState("#0EA5E9");
  return (
    <ColorSwatch
      ariaLabel="Accent colour"
      isDisabled={isDisabled}
      isLocked={isLocked}
      onChange={setValue}
      value={value}
    />
  );
}

export const Default: Story = {
  args: { ariaLabel: "Accent colour", value: "#0EA5E9" },
  render: () => <SwatchExample />,
};

export const Locked: Story = {
  args: { ariaLabel: "Accent colour", value: "#0EA5E9" },
  render: () => <SwatchExample isLocked />,
};

export const Disabled: Story = {
  args: { ariaLabel: "Accent colour", value: "#0EA5E9" },
  render: () => <SwatchExample isDisabled />,
};
