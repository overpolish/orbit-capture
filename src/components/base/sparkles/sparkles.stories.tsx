import { Meta, StoryObj } from "@storybook/react";
import { Hand, Sparkle } from "lucide-react";

import { Sparkles } from "./sparkles";

const meta = {
  argTypes: {
    children: { control: { disable: true } },
    fillType: {
      control: "inline-radio",
      options: ["fill-only", "stroke-only", "fill-and-stroke"],
      table: { defaultValue: { summary: "fill-only" } },
    },
    icon: { control: { disable: true } },
  },
  args: {
    children: (
      <span className="font-bold text-3xl text-content-fg">sparkles</span>
    ),
    fillType: "fill-only",
    icon: Sparkle,
  },
  component: Sparkles,
  parameters: {
    controls: { include: ["fillType"] },
    layout: "centered",
  },
  title: "Sparkles",
} satisfies Meta<typeof Sparkles>;

export default meta;
type Story = StoryObj<typeof meta>;

/* --------------------------------- Stories -------------------------------- */
export const Default: Story = {};

export const Colors: Story = {
  args: {
    colors: ["#F00", "#0F0", "#00F"],
  },
};

export const Icon: Story = {
  args: {
    fillType: "stroke-only",
    icon: Hand,
  },
};

export const Scale: Story = {
  args: {
    scale: { max: 4, min: 2 },
  },
};

export const Duration: Story = {
  args: {
    duration: { max: 5, min: 3 },
  },
};

export const Opacity: Story = {
  args: {
    opacity: 0.5,
  },
};

export const MoreSparkles: Story = {
  args: {
    sparklesCount: 50,
  },
};
