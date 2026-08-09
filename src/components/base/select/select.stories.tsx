// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Meta, StoryObj } from "@storybook/react";
import { Volume2 } from "lucide-react";

import { ListBoxItem } from "../listbox-item/listbox-item";

import { Select } from "./select";

const sizes: React.ComponentProps<typeof Select>["size"][] = [
  "md",
  "sm",
] as const;
const variants: React.ComponentProps<typeof Select>["variant"][] = [
  "solid",
  "ghost",
  "line",
] as const;
const isolatedDocsStory = {
  docs: {
    story: {
      height: "240px",
      inline: false,
    },
  },
} as const;

const meta = {
  args: {
    children: (
      <>
        <ListBoxItem textValue="Chocolate">Chocolate</ListBoxItem>
        <ListBoxItem textValue="Vanilla">Vanilla</ListBoxItem>
      </>
    ),
    className: "w-[180px]",
    label: "🥤 Flavour",
  },
  component: Select,
  parameters: {
    controls: { include: ["label"] },
    layout: "centered",
  },
  title: "Select",
} satisfies Meta<typeof Select>;

export default meta;
type Story = StoryObj<typeof meta>;

/* --------------------------------- Stories -------------------------------- */
export const Default: Story = {};

export const Clearable: Story = {
  args: {
    clearable: true,
  },
  parameters: isolatedDocsStory,
};

export const Sizes: Story = {
  parameters: {
    controls: { disable: true },
    ...isolatedDocsStory,
  },
  render: (args) => (
    <div className="flex gap-2 items-center">
      {sizes.map((size) => (
        <Select key={size} size={size} {...args}>
          <ListBoxItem size={size} textValue="Chocolate">
            Chocolate
          </ListBoxItem>
          <ListBoxItem size={size} textValue="Vanilla">
            Vanilla
          </ListBoxItem>
        </Select>
      ))}
    </div>
  ),
};

export const Variants: Story = {
  parameters: {
    controls: { disable: true },
    ...isolatedDocsStory,
  },
  render: (args) => (
    <div className="flex gap-2 items-center">
      {variants.map((variant) => (
        <Select key={variant} variant={variant} {...args}>
          <ListBoxItem textValue="Chocolate">Chocolate</ListBoxItem>
          <ListBoxItem textValue="Vanilla">Vanilla</ListBoxItem>
        </Select>
      ))}
    </div>
  ),
};

export const LeftSection: Story = {
  args: {
    leftSection: <Volume2 size={14} />,
  },
  parameters: isolatedDocsStory,
};

export const LongValue: Story = {
  args: {
    children: (
      <>
        <ListBoxItem>
          This is a really long label which should overflow
        </ListBoxItem>
      </>
    ),
    leftSection: <Volume2 size={14} />,
  },
  parameters: isolatedDocsStory,
};

export const DefaultValue: Story = {
  args: {
    children: (
      <>
        <ListBoxItem id="Chocolate">Chocolate</ListBoxItem>
        <ListBoxItem id="Vanilla">Vanilla</ListBoxItem>
      </>
    ),
    clearable: false,
    selectedKey: "Vanilla",
  },
  parameters: isolatedDocsStory,
};
