// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Checkbox } from "./checkbox";

import type { Meta, StoryObj } from "@storybook/react-vite";

const sizes: NonNullable<React.ComponentProps<typeof Checkbox>["size"]>[] = [
  "md",
  "sm",
  "xs",
] as const;

const meta = {
  argTypes: {
    size: {
      control: "inline-radio",
      options: sizes,
      table: { defaultValue: { summary: "md" } },
    },
  },
  args: {
    children: "Label",
  },
  component: Checkbox,
  parameters: {
    controls: {
      controls: ["className"],
    },
    layout: "centered",
  },
  title: "Checkbox",
} satisfies Meta<typeof Checkbox>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    size: "md",
  },
};

export const Sizes: Story = {
  parameters: {
    controls: { disable: true },
  },
  render: (args) => (
    <div className="flex flex-col items-center gap-4">
      {sizes.map((size) => (
        <Checkbox key={size} size={size} {...args} />
      ))}
    </div>
  ),
};

export const Disabled: Story = {
  args: {
    isDisabled: true,
    isSelected: true,
  },
  parameters: {
    controls: {
      exclude: ["children"],
    },
  },
};
