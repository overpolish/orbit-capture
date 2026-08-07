import { Meta, StoryObj } from "@storybook/react";

import { ToggleButton } from "./toggle-button";

const sizes: React.ComponentProps<typeof ToggleButton>["size"][] = [
  "md",
  "sm",
] as const;

const variants: React.ComponentProps<typeof ToggleButton>["variant"][] = [
  "solid",
  "ghost",
] as const;

/** Toggle button - provide `off` for a separate state when disabled */
const meta = {
  argTypes: {
    size: {
      control: "inline-radio",
      options: sizes,
      table: { defaultValue: { summary: "md" } },
    },
    variant: {
      control: "inline-radio",
      options: variants,
      table: { defaultValue: { summary: "solid" } },
    },
  },
  args: {
    children: "Pin",
  },
  component: ToggleButton,
  parameters: {
    controls: {
      exclude: ["className", "off", "children"],
    },
    layout: "centered",
  },
  title: "Toggle Button",
} satisfies Meta<typeof ToggleButton>;

export default meta;
type Story = StoryObj<typeof meta>;

/* --------------------------------- Stories -------------------------------- */
export const Default: Story = {
  args: { size: "md", variant: "solid" },
};

export const Sizes: Story = {
  parameters: {
    controls: { disable: true },
  },
  render: (args) => (
    <div className="flex gap-2 items-center">
      {sizes.map((size) => (
        <ToggleButton key={size} size={size} {...args} />
      ))}
    </div>
  ),
};

export const Variants: Story = {
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div className="flex gap-4 items-center">
      {variants.map((variant) => (
        <ToggleButton key={variant} variant={variant} {...args} />
      ))}
    </div>
  ),
};

export const CustomOff: Story = {
  args: {
    className: "w-16",
    off: "Unpin",
    variant: "ghost",
  },
  parameters: { controls: { include: ["variant"] } },
};
