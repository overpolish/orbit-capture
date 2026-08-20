// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { type Meta, type StoryObj } from "@storybook/react-vite";
import { useSyncExternalStore, type ReactNode } from "react";

import { UpdatePrompt } from "./update-prompt";

const previewWidth = 620;
const previewHeight = 520;
const previewPadding = 24;

const getPreviewScale = () =>
  Math.max(
    Math.min(
      (window.innerWidth - previewPadding * 2) / previewWidth,
      (window.innerHeight - previewPadding * 2) / previewHeight,
      1,
    ),
    0.1,
  );

const subscribeToPreviewSize = (onStoreChange: () => void) => {
  const onResize = () => {
    onStoreChange();
  };
  window.addEventListener("resize", onResize);
  return () => {
    window.removeEventListener("resize", onResize);
  };
};

function UpdatePromptPreviewFrame({ children }: { children: ReactNode }) {
  const scale = useSyncExternalStore(
    subscribeToPreviewSize,
    getPreviewScale,
    () => 1,
  );

  return (
    <div className="fixed inset-0 flex items-center justify-center overflow-hidden">
      <div
        className="shrink-0 overflow-hidden shadow-2xl"
        style={{
          height: previewHeight,
          transform: `scale(${String(scale)})`,
          width: previewWidth,
        }}
      >
        {children}
      </div>
    </div>
  );
}

const meta = {
  args: {
    currentVersion: "0.1.0",
    downloadProgress: null,
    error: null,
    onInstall: () => undefined,
    onRemindLater: () => undefined,
    onSkipVersion: () => undefined,
    releaseDate: "2026-08-18T12:00:00Z",
    releaseNotes:
      '<ul><li>Capture windows and regions more reliably.</li><li>Added smoother cursor movement to exported recordings.</li><li>Remembered the last selected microphone and camera.</li><li>Improved export performance for <strong>longer recordings</strong>.</li><li>Added clearer feedback while preparing an export.</li><li>Improved recording controls on smaller displays.</li><li>Fixed occasional blank frames at the start of recordings.</li><li>Fixed window capture when an application changes size.</li><li>Fixed keyboard shortcuts after waking the computer.</li><li>Updated translations and <a href="https://github.com/overpolish/screenwide">accessibility labels</a>.</li></ul>',
    status: "available" as const,
    updateVersion: "0.2.0",
  },
  component: UpdatePrompt,
  decorators: [
    (Story, context) =>
      context.viewMode === "docs" ? (
        <div className="h-[520px] w-[620px] max-w-full overflow-hidden">
          <Story />
        </div>
      ) : (
        <UpdatePromptPreviewFrame>
          <Story />
        </UpdatePromptPreviewFrame>
      ),
  ],
  parameters: { layout: "centered" },
  title: "Features/Update Prompt",
} satisfies Meta<typeof UpdatePrompt>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Available: Story = {};

export const Installing: Story = {
  args: {
    downloadProgress: 0.62,
    status: "downloading",
  },
};

export const InstallFailure: Story = {
  args: {
    error: "The downloaded update could not be verified.",
    status: "error",
  },
};
