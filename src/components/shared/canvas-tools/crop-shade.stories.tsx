// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { CropShade } from "./crop-shade";

import type { Meta, StoryObj } from "@storybook/react-vite";

const meta = {
  component: CropShade,
  decorators: [
    (Story) => (
      <div className="relative h-64 w-96 overflow-visible bg-gradient-to-br from-cyan-500 to-fuchsia-600">
        <Story />
      </div>
    ),
  ],
  parameters: { layout: "centered" },
  title: "Crop Shade",
} satisfies Meta<typeof CropShade>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    crop: { height: 140, width: 230, x: 82, y: 62 },
    image: { height: 230, width: 360, x: 18, y: 17 },
    radius: 30,
  },
};
