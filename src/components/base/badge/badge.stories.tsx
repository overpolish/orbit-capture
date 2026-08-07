import { Check } from "lucide-react";
import { ComponentProps } from "react";

import { Badge } from "./badge";

import type { Meta, StoryObj } from "@storybook/react-vite";

const sizes: ComponentProps<typeof Badge>["size"][] = ["md", "sm"];
const colors: ComponentProps<typeof Badge>["color"][] = [
  "neutral",
  "info",
  "warning",
  "error",
];
const variants: ComponentProps<typeof Badge>["variant"][] = [
  "outline",
  "ghost",
];

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
    variant: {
      control: "inline-radio",
      options: variants,
      table: { defaultValue: { summary: "outline" } },
    },
  },
  args: {
    children: <>Default</>,
    color: "neutral",
    size: "md",
    variant: "outline",
  },
  component: Badge,
  parameters: {
    controls: { exclude: ["children", "className"] },
    layout: "centered",
  },
  title: "Badge",
} satisfies Meta<typeof Badge>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Colors: Story = {
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div className="flex items-center gap-2">
      {colors.map((color) => (
        <div className="flex flex-col items-center gap-1" key={color}>
          <Badge {...args} color={color} />
          <span className="text-xs text-muted">{color}</span>
        </div>
      ))}
    </div>
  ),
};

export const Sizes: Story = {
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div className="flex items-end gap-2">
      {sizes.map((size) => (
        <div className="flex flex-col items-center gap-1" key={size}>
          <Badge {...args} size={size} />
          <span className="text-xs text-muted">{size}</span>
        </div>
      ))}
    </div>
  ),
};

export const Variants: Story = {
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div className="flex items-center gap-2">
      {variants.map((variant) => (
        <div className="flex flex-col items-center gap-1" key={variant}>
          <Badge {...args} variant={variant} />
          <span className="text-xs text-muted">{variant}</span>
        </div>
      ))}
    </div>
  ),
};

export const WithIcon: Story = {
  args: {
    children: (
      <>
        <Check size={20} />
        Default
      </>
    ),
  },
};
