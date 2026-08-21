// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ComponentProps } from "react";

import { Point } from "./pixel-analysis";
import { LABEL_HEIGHT } from "./ruler-label-metrics";
import { SvgLabel } from "./ruler-svg-label";
import { useLabelHandles } from "./use-label-handles";

import type { Meta, StoryObj } from "@storybook/react-vite";

const STAGE = { height: 120, width: 320 };

const meta = {
  args: { text: "128 px", x: STAGE.width / 2, y: STAGE.height / 2 },
  component: SvgLabel,
  decorators: [
    (Story) => (
      <div className="rounded-md bg-neutral-100 p-2">
        <svg height={STAGE.height} width={STAGE.width}>
          <Story />
        </svg>
      </div>
    ),
  ],
  parameters: { layout: "centered" },
  title: "Features/Ruler Label",
} satisfies Meta<typeof SvgLabel>;

export default meta;
type Story = StoryObj<typeof meta>;

/* --------------------------------- Stories -------------------------------- */

/** The tooltip-toned chip every measurement, probe and gap uses. */
export const Default: Story = {};

const samples = ["8 px", "128 px", "1440 × 900 px"] as const;

/**
 * Chip width is derived from the text length, so the chip has to stay legible
 * from a two-character value up to a full two-axis readout.
 */
export const Lengths: Story = {
  parameters: { controls: { disable: true } },
  render: (args) => (
    <>
      {samples.map((text, index) => (
        <SvgLabel
          key={text}
          {...args}
          text={text}
          x={STAGE.width / 2}
          y={24 + index * (LABEL_HEIGHT + 12)}
        />
      ))}
    </>
  ),
};

const identityToWorld = (point: Point) => point;
const noopRecord = () => undefined;

/**
 * The real `useLabelHandles`, wired to an identity `toWorld` so client px are
 * world px — the same pointer-capture path the ruler window uses.
 */
function DraggableLabel(props: ComponentProps<typeof SvgLabel>) {
  const { handles } = useLabelHandles(identityToWorld, noopRecord);
  return <SvgLabel {...props} handles={handles} labelKey="m1" />;
}

/**
 * With `handles` and a `labelKey` the chip re-enables pointer events on itself,
 * shows the move cursor and drags: the offset it accumulates is what moves it.
 */
export const Interactive: Story = {
  args: { text: "Drag me" },
  render: (args) => <DraggableLabel {...args} />,
};
