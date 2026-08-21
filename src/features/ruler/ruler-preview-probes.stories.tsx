// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { PreviewProbeLayer } from "./ruler-preview-probes";
import { RulerStoryStage } from "./ruler-story-stage";
import { DistanceProbe } from "./ruler-types";

import type { Meta, StoryObj } from "@storybook/react-vite";

/**
 * The pair crosses at (320, 200): the horizontal probe's cut-out sits at the
 * vertical probe's x, and vice versa, each 3px wide.
 */
const probes: readonly DistanceProbe[] = [
  { axis: "x", end: 560, position: 200, start: 80 },
  { axis: "y", end: 340, position: 320, start: 60 },
];

const meta = {
  args: { probes, toScreen: (point) => point },
  component: PreviewProbeLayer,
  parameters: { layout: "padded" },
  render: (args) => (
    <RulerStoryStage>
      <PreviewProbeLayer {...args} />
    </RulerStoryStage>
  ),
  title: "Features/Ruler Preview Probes",
} satisfies Meta<typeof PreviewProbeLayer>;

export default meta;
type Story = StoryObj<typeof meta>;

/* --------------------------------- Stories -------------------------------- */

/** Both axes, with the intersection cut out of each stroke. */
export const Crossing: Story = {};

/** A lone axis has nothing to cross, so it renders unbroken. */
export const SingleAxis: Story = {
  args: { probes: [probes[0]] },
};
