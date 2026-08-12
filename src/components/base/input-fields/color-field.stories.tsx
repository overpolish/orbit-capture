// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { type Meta, type StoryObj } from "@storybook/react-vite";
import { useState } from "react";

import { ColorField } from "./color-field";

const meta = {
  component: ColorField,
  parameters: { layout: "centered" },
  title: "Color Field",
} satisfies Meta<typeof ColorField>;

export default meta;
type Story = StoryObj<typeof meta>;

function ControlledColorField(args: React.ComponentProps<typeof ColorField>) {
  const [value, setValue] = useState(args.value);
  return <ColorField {...args} onChange={setValue} value={value} />;
}

export const Default: Story = {
  args: { label: "Background", value: "#172554" },
  render: (args) => <ControlledColorField {...args} />,
};

export const ShortHex: Story = {
  args: { label: "Background", value: "#123" },
  render: (args) => <ControlledColorField {...args} />,
};
