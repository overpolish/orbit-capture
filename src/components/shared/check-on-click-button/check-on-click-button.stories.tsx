import { Meta, StoryObj } from "@storybook/react";

import { CheckOnClickButton } from "./check-on-click-button";

const meta = {
  args: {
    children: "Action",
  },
  component: CheckOnClickButton,
  parameters: {
    layout: "centered",
  },
  title: "Shared/Check On Click Button",
} satisfies Meta<typeof CheckOnClickButton>;

export default meta;
type Story = StoryObj<typeof meta>;

/* --------------------------------- Stories -------------------------------- */
export const Default: Story = {};
