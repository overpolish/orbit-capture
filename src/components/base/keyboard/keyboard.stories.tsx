import { Meta, StoryObj } from "@storybook/react";

import { Keyboard } from "./keyboard";

const sizes: React.ComponentProps<typeof Keyboard>["size"][] = [
  "md",
  "sm",
  "xs",
] as const;
const variants: React.ComponentProps<typeof Keyboard>["variant"][] = [
  "default",
  "ghost",
] as const;

const meta = {
  argTypes: {
    size: {
      control: "inline-radio",
      options: sizes,
      table: {
        defaultValue: { summary: "md" },
      },
    },
    variant: {
      control: "inline-radio",
      options: variants,
      table: {
        defaultValue: { summary: "default" },
      },
    },
  },
  component: Keyboard,
  parameters: {
    controls: { exclude: ["children", "ref"] },
    layout: "centered",
  },
  title: "Keyboard",
} satisfies Meta<typeof Keyboard>;

export default meta;
type Story = StoryObj<typeof meta>;

/* --------------------------------- Stories -------------------------------- */
export const Default: Story = {
  args: {
    children: (
      <>
        <span>⌘</span>1
      </>
    ),
    size: "md",
    variant: "default",
  },
};

export const Sizes: Story = {
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div className="flex gap-2 items-center">
      {sizes.map((size) => (
        <Keyboard key={size} size={size} {...args}>
          ⇧1
        </Keyboard>
      ))}
    </div>
  ),
};

export const Variants: Story = {
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div className="flex gap-2 items-center">
      {variants.map((variant) => (
        <Keyboard key={variant} variant={variant} {...args}>
          ⇧1
        </Keyboard>
      ))}
    </div>
  ),
};
