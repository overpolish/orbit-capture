import { Meta, StoryObj } from "@storybook/react";

import { ListBoxItem } from "../listbox-item/listbox-item";

import { ListBox } from "./listbox";

const meta = {
  argTypes: {
    selectionMode: {
      control: "inline-radio",
      options: ["single", "multiple"],
      table: { defaultValue: { summary: "solid" } },
    },
  },
  args: {
    className: "w-[180px]",
    selectionMode: "single",
  },
  component: ListBox,
  parameters: {
    controls: { exclude: ["className", "children", "ref"] },
    layout: "centered",
  },
  title: "ListBox",
} satisfies Meta<typeof ListBox>;

export default meta;
type Story = StoryObj<typeof meta>;

/* --------------------------------- Stories -------------------------------- */
export const Default: Story = {
  args: {
    "aria-label": "Milkshake flavor",
    children: (
      <>
        <ListBoxItem textValue="Chocolate">Chocolate</ListBoxItem>
        <ListBoxItem textValue="Strawberry">Strawberry</ListBoxItem>
      </>
    ),
  },
};

export const Compact: Story = {
  args: {
    "aria-label": "Milkshake flavor",
    children: (
      <>
        <ListBoxItem compact textValue="Chocolate">
          Chocolate
        </ListBoxItem>
        <ListBoxItem compact textValue="Strawberry">
          Strawberry
        </ListBoxItem>
      </>
    ),
  },
};

export const Empty: Story = {
  parameters: { controls: { disable: true } },
};

export const LongValue: Story = {
  args: {
    "aria-label": "Long value",
    children: (
      <ListBoxItem>
        This is a really long label which should overflow
      </ListBoxItem>
    ),
  },
  parameters: { controls: { disable: true } },
};
