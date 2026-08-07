import { Meta, StoryObj } from "@storybook/react";

import { Switch } from "./switch";

const sizes: React.ComponentProps<typeof Switch>["size"][] = ["md", "xs"];

const meta = {
  argTypes: {
    className: { control: { disable: true } },
    size: {
      control: "inline-radio",
      options: sizes,
      table: { defaultValue: { summary: "md" } },
    },
  },
  args: { children: "Label" },
  component: Switch,
  parameters: {
    controls: { include: ["size"] },
    layout: "centered",
  },
  title: "Switch",
} satisfies Meta<typeof Switch>;

export default meta;
type Story = StoryObj<typeof meta>;

/* --------------------------------- Stories -------------------------------- */
export const Default: Story = {
  args: { size: "md" },
};

export const Sizes: Story = {
  parameters: {
    controls: { disable: true },
  },
  render: (args) => (
    <div className="flex gap-6 items-center">
      {sizes.map((size) => (
        <Switch key={size} size={size} {...args} />
      ))}
    </div>
  ),
};
