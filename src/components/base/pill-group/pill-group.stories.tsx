// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import type { ComponentProps } from "react";

import { Mic, Volume2 } from "lucide-react";
import { useState } from "react";

import { PillGroup } from "./pill-group";

import type { Meta, StoryObj } from "@storybook/react-vite";

const items = [
  { icon: <Volume2 size={15} />, id: "system", label: "System audio" },
  { icon: <Mic size={15} />, id: "microphone", label: "Microphone" },
];

const meta = {
  component: PillGroup,
  parameters: { layout: "centered" },
  title: "Pill Group",
} satisfies Meta<typeof PillGroup>;

export default meta;
type Story = StoryObj<typeof meta>;

function PillGroupExample(props: ComponentProps<typeof PillGroup>) {
  const [selected, setSelected] = useState(props.selected);
  return (
    <PillGroup {...props} onSelectionChange={setSelected} selected={selected} />
  );
}

export const Icons: Story = {
  args: {
    ariaLabel: "Audio lanes",
    items,
    onSelectionChange: () => undefined,
    selected: "system",
  },
  render: (args) => <PillGroupExample {...args} />,
};

export const IconAndText: Story = {
  args: { ...Icons.args, display: "icon-label" },
  render: (args) => <PillGroupExample {...args} />,
};

export const Text: Story = {
  args: { ...Icons.args, display: "label" },
  render: (args) => <PillGroupExample {...args} />,
};
