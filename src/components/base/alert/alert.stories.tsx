// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ComponentProps } from "react";

import { Alert } from "./alert";

import type { Meta, StoryObj } from "@storybook/react-vite";

const colors: ComponentProps<typeof Alert>["color"][] = [
  "neutral",
  "info",
  "success",
  "warning",
  "error",
];
const sizes: ComponentProps<typeof Alert>["size"][] = ["md", "sm"];

const meta = {
  argTypes: {
    color: {
      control: "inline-radio",
      options: colors,
      table: { defaultValue: { summary: "neutral" } },
    },
    size: {
      control: "inline-radio",
      options: sizes,
      table: { defaultValue: { summary: "md" } },
    },
  },
  args: {
    children: "No additional options are available for this recording.",
    color: "neutral",
    size: "md",
  },
  component: Alert,
  parameters: {
    controls: { exclude: ["children", "className", "icon"] },
    layout: "centered",
  },
  title: "Alert",
} satisfies Meta<typeof Alert>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  decorators: [
    (Story) => (
      <div className="w-80">
        <Story />
      </div>
    ),
  ],
};

export const Colors: Story = {
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div className="flex w-80 flex-col gap-2">
      {colors.map((color) => (
        <Alert key={color} {...args} color={color}>
          {color?.[0].toUpperCase()}
          {color?.slice(1)} alert
        </Alert>
      ))}
    </div>
  ),
};

export const Sizes: Story = {
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div className="flex w-80 flex-col gap-2">
      {sizes.map((size) => (
        <Alert key={size} {...args} size={size} />
      ))}
    </div>
  ),
};
