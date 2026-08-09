// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Meta, StoryObj } from "@storybook/react";
import { Cog } from "lucide-react";

import { CircularProgressBar } from "./circular-progress-bar";

const meta = {
  argTypes: {
    isIndeterminate: { control: "boolean" },
    size: {
      control: { max: 200, min: 14, step: 1, type: "range" },
    },
    strokeWidth: {
      control: { max: 30, min: 1, step: 1, type: "range" },
    },
    value: {
      control: { max: 100, min: 0, step: 1, type: "range" },
    },
  },
  args: {
    "aria-label": "Example progress",
    size: 100,
  },
  component: CircularProgressBar,
  parameters: {
    controls: { exclude: ["aria-label", "renderLabel"] },
    layout: "centered",
  },
  title: "Circular Progress Bar",
} satisfies Meta<typeof CircularProgressBar>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: { strokeWidth: 10, value: 10 },
};

/** Zero is rendered as a real empty progress ring rather than disappearing. */
export const Empty: Story = {
  args: { value: 0 },
};

export const Indeterminate: Story = {
  args: { isIndeterminate: true },
};

/** Position custom content with `renderLabel`. */
export const CustomLabel: Story = {
  args: {
    renderLabel: (percentage) => (
      <>
        <div className="absolute inset-0 flex items-center justify-center">
          <Cog className="animate-spin text-content-fg" size={50} />
        </div>
        <span className="absolute right-0 bottom-0 font-bold text-content-fg">
          {percentage?.toLocaleString()}
        </span>
      </>
    ),
    value: 25,
  },
};
