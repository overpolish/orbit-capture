// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  CursorReadout,
  RulerCrosshair,
  ToleranceIndicator,
} from "./ruler-cursor-overlays";
import { RulerStoryStage } from "./ruler-story-stage";
import { RulerTolerance } from "./ruler-tolerance";
import { DistanceProbe } from "./ruler-types";

import type { Meta, StoryObj } from "@storybook/react-vite";

const CURSOR = { x: 120, y: 70 };

/** A 220 × 120 span under the cursor, the shape the readout reports. */
const probes: readonly DistanceProbe[] = [
  { axis: "x", end: 260, position: 70, start: 40 },
  { axis: "y", end: 150, position: 120, start: 30 },
];

const meta = {
  args: { copied: false, cursor: CURSOR, probes },
  component: CursorReadout,
  parameters: { layout: "padded" },
  render: (args) => (
    <RulerStoryStage size="cursor">
      <CursorReadout {...args} />
    </RulerStoryStage>
  ),
  title: "Features/Ruler Cursor Overlays",
} satisfies Meta<typeof CursorReadout>;

export default meta;
type Story = StoryObj<typeof meta>;

/* --------------------------------- Stories -------------------------------- */

/** Dimensions on top, the sampled pixel colour beneath. */
export const TwoLine: Story = {
  args: { hex: "#3B82F6" },
};

/** No pixel sampled yet — the size line stands alone. */
export const DimensionsOnly: Story = {};

/** Hovering without a measured span leaves only the swatch row. */
export const ColorOnly: Story = {
  args: { hex: "#F8F9FA", probes: [] },
};

/** Tab copies the hex; the colour line acknowledges it in place. */
export const Copied: Story = {
  args: { copied: true, hex: "#3B82F6" },
};

/**
 * Near the bottom of the window the chip flips above the cursor, anchored by
 * its bottom edge. The threshold is measured against the window, so this story
 * fills the viewport and parks the cursor 24px from its foot.
 */
export const FlippedNearBottom: Story = {
  args: { hex: "#3B82F6" },
  parameters: { layout: "fullscreen" },
  render: (args) => {
    const cursor = { x: 120, y: window.innerHeight - 24 };
    return (
      <RulerStoryStage fullHeight size="cursor">
        <CursorReadout {...args} cursor={cursor} />
      </RulerStoryStage>
    );
  },
};

const tolerances: RulerTolerance[] = ["low", "medium", "high"];

/** The transient chip shown while the detector tolerance is cycled. */
export const Tolerance: StoryObj<typeof ToleranceIndicator> = {
  argTypes: {
    tolerance: { control: "inline-radio", options: tolerances },
  },
  args: { cursor: CURSOR, tolerance: "medium" },
  render: (args) => (
    <RulerStoryStage size="cursor">
      <ToleranceIndicator {...args} />
    </RulerStoryStage>
  ),
};

/** The full-bleed sight lines the cursor drags around the screen. */
export const Crosshair: StoryObj<typeof RulerCrosshair> = {
  args: { cursor: CURSOR },
  render: (args) => (
    <RulerStoryStage size="cursor">
      <RulerCrosshair {...args} />
    </RulerStoryStage>
  ),
};
