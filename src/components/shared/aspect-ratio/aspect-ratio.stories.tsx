import { Meta, StoryObj } from "@storybook/react";

import { AspectRatio } from "./aspect-ratio";

const meta = {
  component: AspectRatio,
  parameters: {
    layout: "centered",
  },
  title: "Shared/Aspect Ratio",
} satisfies Meta<typeof AspectRatio>;

export default meta;
type Story = StoryObj<typeof meta>;

/* --------------------------------- Stories -------------------------------- */
export const Default: Story = {
  args: {
    onApply: (width, height) => {
      console.log("Apply pressed", { height, width });
    },
  },
};
