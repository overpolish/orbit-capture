// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useState } from "react";

import { ColorPaletteGenerator } from "./color-palette-generator";

import type { Meta, StoryObj } from "@storybook/react-vite";

const initialColors = ["#164E63", "#0891B2", "#22D3EE", "#A5F3FC"];

const meta = {
  component: ColorPaletteGenerator,
  parameters: { layout: "centered" },
  title: "Color Palette Generator",
} satisfies Meta<typeof ColorPaletteGenerator>;

export default meta;
type Story = StoryObj<typeof meta>;

function PaletteExample({
  initialLocked = initialColors.map(() => false),
  isDisabled,
}: {
  initialLocked?: boolean[];
  isDisabled?: boolean;
}) {
  const [colors, setColors] = useState(initialColors);
  const [locked, setLocked] = useState(initialLocked);
  return (
    <div className="w-64">
      <ColorPaletteGenerator
        colors={colors}
        isDisabled={isDisabled}
        locked={locked}
        onChange={setColors}
        onLockedChange={setLocked}
      />
    </div>
  );
}

export const Default: Story = {
  args: { colors: initialColors },
  render: () => <PaletteExample />,
};

export const WithLockedColours: Story = {
  args: { colors: initialColors },
  render: () => <PaletteExample initialLocked={[false, true, false, true]} />,
};

export const Disabled: Story = {
  args: { colors: initialColors },
  render: () => <PaletteExample isDisabled />,
};
