import { Meta, StoryObj } from "@storybook/react";
import { IceCream } from "lucide-react";
import { ReactNode } from "react";

import { Button } from "../button/button";

import { ContentRotate } from "./content-rotate";

const childrenOptions: Record<string, ReactNode> = {
  textOnly: "Chocolate",
  withIcon: (
    <>
      <IceCream size={18} />
      Vanilla
    </>
  ),
};

/** Styling is left to consumer */
const meta: Meta<typeof ContentRotate> = {
  argTypes: {
    contentKey: {
      control: {
        labels: {
          textOnly: "Text Only",
          withIcon: "With Icon",
        },
        type: "inline-radio",
      },
      options: Object.keys(childrenOptions),
    },
  },
  args: {
    children: childrenOptions.withIcon,
    className: "text-content-fg flex gap-2 items-center",
    contentKey: "textOnly",
  },
  component: ContentRotate,
  parameters: {
    controls: { exclude: ["className", "children"] },
    layout: "centered",
  },
  title: "Word Rotate",
};

export default meta;
type Story = StoryObj<typeof ContentRotate>;

/* --------------------------------- Stories -------------------------------- */
export const Default: Story = {
  render: ({ children: _children, ...args }) => (
    <ContentRotate {...args}>{childrenOptions[args.contentKey]}</ContentRotate>
  ),
};

/** Recommendation is to set a defined width. */
export const InButton: Story = {
  render: ({ children: _children, ...args }) => (
    <Button className="w-25 justify-center">
      <ContentRotate {...args}>
        {childrenOptions[args.contentKey]}
      </ContentRotate>
    </Button>
  ),
};
