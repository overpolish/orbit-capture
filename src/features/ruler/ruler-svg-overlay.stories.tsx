// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ComponentProps, useState } from "react";

import { Button } from "../../components/base/button/button";

import { RulerComponentBox } from "./api";
import { RulerStoryStage } from "./ruler-story-stage";
import { RulerSvgOverlay } from "./ruler-svg-overlay";
import { DistanceProbe, Measurement } from "./ruler-types";
import { LabelHandles } from "./use-label-handles";

import type { Meta, StoryObj } from "@storybook/react-vite";

/** Inert stand-in for `useLabelHandles` — chips render, nothing drags. */
const handles: LabelHandles = {
  beginDrag: () => undefined,
  drag: () => undefined,
  endDrag: () => undefined,
  enter: () => undefined,
  leave: () => undefined,
  offset: () => ({ x: 0, y: 0 }),
};

/**
 * Every fixture runs at `deviceScale: 1`, so world px and the detector's device
 * px are the same numbers and the alignment arithmetic below stays readable.
 */
const measurements: readonly Measurement[] = [
  { height: 140, id: 1, width: 240, x: 60, y: 60 },
  // Centred on x=180 like its sibling, which accents both x centerlines; too
  // narrow for its own label, which therefore parks above the box.
  { height: 40, id: 2, width: 70, x: 145, y: 250 },
];

/**
 * Detector output in device px. The boxes matching a measurement edge-for-edge
 * are dropped by the self-exclusion, so only the three genuine pieces of
 * content inside measurement 1 become inner objects:
 *
 * - `150..210 × 80..110` centres on x=180 — the measurement's own x centre — so
 *   it draws a vertical centre tick.
 * - `250..290 × 115..145` centres on y=130 — the measurement's y centre — so it
 *   draws a horizontal one.
 * - `80..120 × 150..170` is centred on neither and stays a bare outline.
 *
 * All three sit more than `CLUSTER_GAP` (6px) apart, so none of them merge.
 */
const boxes: readonly RulerComponentBox[] = [
  { height: 140, width: 240, x: 60, y: 60 },
  { height: 30, width: 60, x: 150, y: 80 },
  { height: 30, width: 40, x: 250, y: 115 },
  { height: 20, width: 40, x: 80, y: 150 },
  { height: 40, width: 70, x: 145, y: 250 },
  { height: 24, width: 120, x: 380, y: 70 },
];

const distanceProbes: readonly DistanceProbe[] = [
  { axis: "x", end: 300, id: 11, position: 340, start: 60 },
  { axis: "y", end: 220, id: 12, position: 560, start: 60 },
];

const meta = {
  args: {
    boxes,
    centerlines: true,
    detectedBoxes: false,
    deviceScale: 1,
    distanceProbes,
    handles,
    measurements,
  },
  component: RulerSvgOverlay,
  parameters: { layout: "padded" },
  render: (args) => (
    <RulerStoryStage>
      <RulerSvgOverlay {...args} />
    </RulerStoryStage>
  ),
  title: "Features/Ruler Overlay",
} satisfies Meta<typeof RulerSvgOverlay>;

export default meta;
type Story = StoryObj<typeof meta>;

/* --------------------------------- Stories -------------------------------- */

/**
 * The full gallery: two committed measurements, the larger one selected for
 * deletion, plus two stamped probes.
 */
export const Default: Story = {
  args: { highlighted: { id: 1, kind: "measurement" } },
};

/** The same scene with a probe armed for deletion instead of a box. */
export const SelectedProbe: Story = {
  args: { highlighted: { id: 12, kind: "probe" } },
};

/** Centerlines off — just the boxes, their labels and the probes. */
export const NoCenterlines: Story = {
  args: { centerlines: false },
};

/** Mid-drag: no label, no settle, and the draft gets centerlines of its own. */
export const Draft: Story = {
  args: {
    boxes: [],
    distanceProbes: [],
    draft: { height: 100, width: 180, x: 200, y: 120 },
    measurements: [],
  },
};

/** The KeyB debug view: every component the detector found at this tolerance. */
export const DetectedBoxes: Story = {
  args: { centerlines: false, detectedBoxes: true },
};

const settling: readonly Measurement[] = [
  {
    from: { height: 200, width: 300, x: 40, y: 40 },
    height: 140,
    id: 1,
    width: 240,
    x: 60,
    y: 60,
  },
];

/**
 * A freshly committed measurement eases from the raw drag rect onto its snapped
 * bounds. The animation runs once per mount, so the overlay is remounted to
 * replay it. Inner outlines are withheld until the box lands on them.
 */
function SettlePreview(props: ComponentProps<typeof RulerSvgOverlay>) {
  const [run, setRun] = useState(0);
  return (
    <div className="flex flex-col items-center gap-3">
      <RulerStoryStage key={run}>
        <RulerSvgOverlay {...props} />
      </RulerStoryStage>
      <Button
        onPress={() => {
          setRun((current) => current + 1);
        }}
        size="sm"
        variant="soft"
      >
        Replay settle
      </Button>
    </div>
  );
}

export const SettleAnimation: Story = {
  args: { distanceProbes: [], measurements: settling },
  render: (args) => <SettlePreview {...args} />,
};
