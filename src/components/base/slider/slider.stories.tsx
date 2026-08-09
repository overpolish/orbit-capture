import { Slider } from "./slider";

import type { Meta, StoryObj } from "@storybook/react-vite";

const meta = {
  args: {
    defaultValue: 40,
    label: "Compression",
    maxValue: 100,
    minValue: 0,
    renderValue: (value: number) => `${value.toString()}%`,
  },
  component: Slider,
  decorators: [
    (Story) => (
      <div className="w-80">
        <Story />
      </div>
    ),
  ],
  parameters: {
    layout: "centered",
  },
  title: "Slider",
} satisfies Meta<typeof Slider>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Original: Story = {
  args: {
    defaultValue: 0,
    renderValue: () => "Original",
  },
};

export const Disabled: Story = {
  args: {
    isDisabled: true,
  },
};
