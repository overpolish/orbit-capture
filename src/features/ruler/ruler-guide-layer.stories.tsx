// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { GuideLayer } from "./ruler-guide-layer";
import { RulerStoryStage } from "./ruler-story-stage";
import { Guide } from "./ruler-types";
import { LabelHandles } from "./use-label-handles";

import type { Meta, StoryObj } from "@storybook/react-vite";

const viewport = { height: 400, width: 640 };

/** Inert stand-in for `useLabelHandles` — gap chips render, nothing drags. */
const handles: LabelHandles = {
  beginDrag: () => undefined,
  drag: () => undefined,
  endDrag: () => undefined,
  enter: () => undefined,
  leave: () => undefined,
  offset: () => ({ x: 0, y: 0 }),
};

/**
 * Two 100px gaps and one 60px gap. Every gap clears the quarter-viewport cap
 * (160px on a 640px-wide stage), so all three get a chip.
 */
const verticals: readonly Guide[] = [
  { anchor: 200, axis: "x", id: 1, position: 100 },
  { anchor: 200, axis: "x", id: 2, position: 200 },
  { anchor: 200, axis: "x", id: 3, position: 300 },
  { anchor: 200, axis: "x", id: 4, position: 360 },
];

/** Two equal 80px gaps, inside the 100px cap a 400px-tall stage allows. */
const horizontals: readonly Guide[] = [
  { anchor: 320, axis: "y", id: 5, position: 80 },
  { anchor: 320, axis: "y", id: 6, position: 160 },
  { anchor: 320, axis: "y", id: 7, position: 240 },
];

const meta = {
  args: { guides: verticals, handles, style: {}, viewport },
  component: GuideLayer,
  parameters: { layout: "padded" },
  render: (args) => (
    <RulerStoryStage>
      <GuideLayer {...args} />
    </RulerStoryStage>
  ),
  title: "Features/Ruler Guides",
} satisfies Meta<typeof GuideLayer>;

export default meta;
type Story = StoryObj<typeof meta>;

/* --------------------------------- Stories -------------------------------- */

/** Adjacent gaps each get a chip parked at the pair's placement anchor. */
export const Vertical: Story = {};

/** The same rhythm on the other axis, chips parked on the shared anchor. */
export const Horizontal: Story = {
  args: { guides: horizontals },
};

/** Both axes at once — gaps never compare across axes. */
export const BothAxes: Story = {
  args: { guides: [...verticals, ...horizontals] },
};

/** A pulsing halo marks the guide the delete key would remove. */
export const Selected: Story = {
  args: { selectedId: 2 },
};

/**
 * The guide under the cursor before it is committed: dashed and dimmed, and
 * deliberately outside the gap arithmetic.
 */
export const Preview: Story = {
  args: {
    preview: { anchor: 200, axis: "x", id: 99, position: 480, transient: true },
  },
};
