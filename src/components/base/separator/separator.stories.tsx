import { Meta, StoryObj } from "@storybook/react";

import { Separator } from "./separator";

const meta = {
  argTypes: {
    orientation: {
      control: "inline-radio",
      options: ["horizontal", "vertical"],
      table: { readonly: true },
    },
  },
  component: Separator,
  parameters: {
    controls: { exclude: ["className", "children"] },
    layout: "centered",
  },
  title: "Separator",
} satisfies Meta<typeof Separator>;

export default meta;
type Story = StoryObj<typeof meta>;

/* --------------------------------- Stories -------------------------------- */
export const Horizontal: Story = {
  args: {
    orientation: "vertical",
  },
  render: () => (
    <div className="text-content-fg text-center">
      Hello
      <Separator />
      World!
    </div>
  ),
};

/**
 * The container will need a determined height, otherwise add a class like `h-[10px]`.
 */
export const Vertical: Story = {
  args: {
    orientation: "vertical",
  },
  render: () => (
    <div className="text-content-fg text-center flex">
      Hello
      <Separator className="h-[20px]" orientation="vertical" />
      World!
    </div>
  ),
};

export const WithChildren: Story = {
  args: {
    orientation: "horizontal",
  },
  render: () => (
    <div className="flex items-center gap-4">
      <div className="text-content-fg text-center">
        Hello
        <Separator>
          <span className="text-xs bg-content px-1">or</span>
        </Separator>
        World!
      </div>

      <div className="text-content-fg text-center flex items-center">
        Hello
        <Separator className="h-[30px] mx-4" orientation="vertical">
          <span className="text-xs bg-content px-1">or</span>
        </Separator>
        World!
      </div>
    </div>
  ),
};
