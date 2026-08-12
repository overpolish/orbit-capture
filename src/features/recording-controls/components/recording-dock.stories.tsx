// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { type Meta, type StoryObj } from "@storybook/react-vite";
import { useEffect, useState } from "react";

import { RecordingDock } from "./recording-dock";

/** Ticks a real second at a time so the digit roll can be watched. */
function TickingDock({ fromMs }: { fromMs: number }) {
  const [elapsedMs, setElapsedMs] = useState(fromMs);

  useEffect(() => {
    const interval = window.setInterval(() => {
      setElapsedMs((current) => current + 1_000);
    }, 1_000);

    return () => {
      window.clearInterval(interval);
    };
  }, []);

  return <RecordingDock elapsedMs={elapsedMs} status="recording" />;
}

const meta = {
  component: RecordingDock,
  parameters: {
    layout: "centered",
  },
  title: "Recording/Recording Dock",
} satisfies Meta<typeof RecordingDock>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Idle: Story = {
  args: { status: "idle" },
};

export const Starting: Story = {
  args: { status: "starting" },
};

export const Recording: Story = {
  args: { elapsedMs: 42_000, status: "recording" },
};

export const RecordingWithConfidenceChecks: Story = {
  args: {
    elapsedMs: 42_000,
    monitor: {
      cameraCanvasRef: { current: null },
      hasCamera: true,
      hasCameraFrame: false,
      hasMicrophone: true,
      hasSystemAudio: true,
      microphoneDecibels: -14,
      systemAudioDecibels: -24,
    },
    status: "recording",
  },
};

export const RecordingForHours: Story = {
  args: { elapsedMs: 4_215_000, status: "recording" },
};

/* ---------------------------- Digit boundaries ---------------------------- */

/** 00:00:59, the tick before the minute rolls over. */
export const BeforeMinuteRollover: Story = {
  args: { elapsedMs: 59_000, status: "recording" },
};

/** 00:01:00, straight after the minute rolled over. */
export const AfterMinuteRollover: Story = {
  args: { elapsedMs: 60_000, status: "recording" },
};

/** 00:59:59, the tick before the hour rolls over. */
export const BeforeHourRollover: Story = {
  args: { elapsedMs: 3_599_000, status: "recording" },
};

/** 01:00:00. The hours field is always rendered, so nothing resizes here. */
export const AfterHourRollover: Story = {
  args: { elapsedMs: 3_600_000, status: "recording" },
};

/** Rolls 00:00:57 through the minute boundary, one second at a time. */
export const RollingOverMinute: Story = {
  render: () => <TickingDock fromMs={57_000} />,
};

/** Rolls 00:59:57 through the hour boundary, one second at a time. */
export const RollingOverHour: Story = {
  render: () => <TickingDock fromMs={3_597_000} />,
};

/** The discard button after its first press, waiting to be confirmed. */
export const DiscardArmed: Story = {
  args: { elapsedMs: 42_000, status: "recording" },
  play: ({ canvasElement }) => {
    canvasElement
      .querySelector<HTMLButtonElement>('[aria-label="Discard recording"]')
      ?.click();
  },
};

export const Paused: Story = {
  args: { elapsedMs: 42_000, status: "paused" },
};

export const Stopping: Story = {
  args: { elapsedMs: 42_000, status: "stopping" },
};
