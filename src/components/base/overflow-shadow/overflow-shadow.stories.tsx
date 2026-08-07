import { Meta, StoryObj } from "@storybook/react";

import { OverflowShadow } from "./overflow-shadow";

const shadowRadii: React.ComponentProps<
  typeof OverflowShadow
>["shadowRadius"][] = ["md", "sm"] as const;

const meta = {
  argTypes: {
    shadowRadius: {
      control: "inline-radio",
      options: shadowRadii,
      table: { defaultValue: { summary: "md" } },
    },
  },
  args: {
    children: (
      <>
        Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod
        tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim
        veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea
        commodo consequat.
      </>
    ),
    className: "p-2",
    hideScrollbar: false,
    insetShadow: false,
    shadowRadius: "md",
  },
  component: OverflowShadow,
  parameters: {
    controls: {
      exclude: ["children", "className", "startAtEnd", "orientation"],
    },
    layout: "centered",
  },
  title: "Overflow Shadow",
} satisfies Meta<typeof OverflowShadow>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Vertical: Story = {
  args: {
    orientation: "vertical",
  },
  render: (args) => (
    <div className="text-content-fg w-[150px] h-[100px] relative overflow-hidden">
      <OverflowShadow {...args} />
    </div>
  ),
};

export const Horizontal: Story = {
  args: {
    orientation: "horizontal",
  },
  render: (args) => (
    <div className="text-content-fg w-[150px] whitespace-nowrap relative overflow-hidden">
      <OverflowShadow {...args} />
    </div>
  ),
};

export const HideScrollbar: Story = {
  args: {
    hideScrollbar: true,
    orientation: "vertical",
    shadowRadius: "md",
  },
  render: (args) => (
    <div className="text-content-fg w-[150px] h-[100px] relative overflow-hidden">
      <OverflowShadow {...args} />
    </div>
  ),
};

export const StartAtEnd: Story = {
  args: {
    orientation: "horizontal",
    startAtEnd: true,
  },
  render: (args) => (
    <div className="text-content-fg w-[150px] relative overflow-hidden">
      <OverflowShadow {...args} />
    </div>
  ),
};

export const InsetShadow: Story = {
  args: {
    insetShadow: true,
    orientation: "horizontal",
  },
  render: (args) => (
    <div className="text-content-fg w-[150px] relative overflow-hidden">
      <OverflowShadow {...args} />
    </div>
  ),
};
