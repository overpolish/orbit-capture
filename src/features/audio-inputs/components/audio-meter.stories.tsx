// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Meta, StoryObj } from "@storybook/react";

import { AudioMeter } from "./audio-meter";

const meta: Meta<typeof AudioMeter> = {
  argTypes: {
    decibels: {
      control: { max: 2, min: -65, step: 0.01, type: "range" },
    },
    height: {
      control: { max: 100, min: 5, step: 1, type: "range" },
    },
    orientation: {
      control: "inline-radio",
      options: ["horizontal", "vertical"],
    },
    peak: {
      control: { max: 2, min: -65, step: 0.01, type: "range" },
    },
    width: {
      control: { max: 200, min: 30, step: 1, type: "range" },
    },
  },
  /* eslint-disable sort-keys */
  args: {
    decibels: -10,
    width: 150,
    height: 10,
    peak: -6,
    disabled: false,
    hidePeakTick: false,
    hideTicks: false,
    orientation: "horizontal",
    radius: 2,
  },
  /* eslint-enable sort-keys */
  component: AudioMeter,
  parameters: {
    layout: "centered",
  },
  title: "Audio Meter",
};

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Disabled: Story = {
  args: { disabled: true },
  parameters: {
    controls: { include: ["disabled"] },
  },
};

export const Vertical: Story = {
  args: {
    height: 150,
    orientation: "vertical",
    width: 10,
  },
};

export const TickVisibility: Story = {
  args: {
    hidePeakTick: true,
    hideTicks: true,
  },
  parameters: {
    controls: { include: ["hidePeakTick", "hideTicks"] },
  },
};
