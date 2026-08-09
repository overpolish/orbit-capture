// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Meta, StoryObj } from "@storybook/react";
import { LoaderCircle } from "lucide-react";

import { Overlay } from "./overlay";

const blurs: React.ComponentProps<typeof Overlay>["blur"][] = [
  "lg",
  "md",
  "sm",
  "xs",
] as const;

/**
 * If you are using an acrylic window effect the parent should have a background,
 * else you'll see weird artifacting.
 */
const meta = {
  argTypes: {
    blur: {
      control: "inline-radio",
      options: blurs,
      table: {
        defaultValue: { summary: "md" },
      },
    },
    children: {
      control: { disable: true },
    },
    className: {
      control: { disable: true },
    },
    isOpen: {
      control: "boolean",
      table: {
        defaultValue: { summary: "true" },
      },
    },
  },
  args: { blur: "md", isOpen: true },
  component: Overlay,
  decorators: [
    (Story, context) => {
      if (context.name === "Contained" || context.name === "Blur Strengths")
        return <Story />;

      return (
        <div>
          <Story />
          <div className="text-content-fg text-3xl font-semibold">
            Some content
          </div>
        </div>
      );
    },
  ],
  parameters: {
    controls: { exclude: ["className", "children"] },
    layout: "centered",
  },
  title: "Overlay",
} satisfies Meta<typeof Overlay>;

export default meta;
type Story = StoryObj<typeof meta>;

/* --------------------------------- Stories -------------------------------- */
export const Default: Story = {};

export const WithContent: Story = {
  args: {
    children: (
      <LoaderCircle className="animate-spin text-content-fg" size={32} />
    ),
  },
};

export const BlurStrengths: Story = {
  args: {
    contained: true,
  },
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div className="flex flex-col items-center text-content-fg gap-3">
      {blurs.map((blur) => (
        <div className="flex items-center gap-4" key={blur}>
          {blur}
          <div className="p-4 relative">
            <Overlay {...args} blur={blur} />
            Hidden
          </div>
        </div>
      ))}
    </div>
  ),
};

/** Parent must be relatively positioned. */
export const Contained: Story = {
  args: {
    contained: true,
  },
  render: (args) => (
    <div className="flex flex-col items-center text-content-fg gap-3">
      <div className="p-2">Normal Content</div>
      <div className="p-2 relative">
        <Overlay {...args} />
        Hidden
      </div>
    </div>
  ),
};
