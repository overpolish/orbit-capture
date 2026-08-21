// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { type Meta, type StoryObj } from "@storybook/react-vite";
import { useSyncExternalStore, type ReactNode } from "react";

import { UpdatePrompt } from "./update-prompt";

const previewWidth = 620;
const previewHeight = 520;
const previewPadding = 24;

// Mirrors GitHub's sanitized `body_html` for the v0.1.0 formatting test
// release. Keep the original attachment URL because GitHub's rendered response
// replaces it with a signed URL that expires after a few minutes.
const githubFormattingReleaseNotes = `
  <h1>Heading 1</h1>
  <h2>Heading 2</h2>
  <h3>Heading 3</h3>
  <h4>Heading 4</h4>
  <h5>Heading 5</h5>
  <h6>Heading 6</h6>
  <ul>
    <li><del>Testing</del> <code>release</code></li>
    <li>
      Some other <strong>item</strong>
      <ul>
        <li><em>Nested</em></li>
      </ul>
    </li>
  </ul>
  <a href="https://github.com/user-attachments/assets/1180102d-d7a1-408c-9a9a-05cc413828df" rel="noopener noreferrer" target="_blank">
    <img alt="image" height="120" src="https://github.com/user-attachments/assets/1180102d-d7a1-408c-9a9a-05cc413828df" width="227" />
  </a>
  <ol>
    <li>Numbered</li>
    <li>List</li>
  </ol>
  <ul class="contains-task-list">
    <li class="task-list-item">
      <input aria-label="Incomplete task" class="task-list-item-checkbox" disabled type="checkbox" />
      Task list
    </li>
    <li class="task-list-item">
      <input aria-label="Completed task" checked class="task-list-item-checkbox" disabled type="checkbox" />
      List
    </li>
  </ul>
  <blockquote>
    <p>Record before you run. This is a blockquote with enough text to demonstrate how longer quoted release notes wrap across lines.</p>
  </blockquote>
  <p>Use <code>CommandOrControl+Shift+R</code> to start recording.</p>
  <pre><code>const recording = await startRecording({
  captureSystemAudio: true,
  showCursor: true,
});</code></pre>
  <p><a href="https://google.com" rel="nofollow">Custom Url</a></p>
  <p>
    <strong>Full Changelog</strong>:
    <a href="https://github.com/overpolish/screenwide/commits/v0.1.0">https://github.com/overpolish/screenwide/commits/v0.1.0</a>
  </p>
`;

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

const applyPreviewTheme = (theme: unknown) => {
  const selectedTheme = theme === "light" ? "light" : "dark";
  document.documentElement.classList.remove("dark", "light");
  document.documentElement.classList.add(selectedTheme);
};

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
    (Story, context) => {
      applyPreviewTheme(context.globals.theme);
      return context.viewMode === "docs" ? (
        <div className="h-[520px] w-[620px] max-w-full overflow-hidden">
          <Story />
        </div>
      ) : (
        <UpdatePromptPreviewFrame>
          <Story />
        </UpdatePromptPreviewFrame>
      );
    },
  ],
  parameters: { layout: "centered" },
  title: "Features/Update Prompt",
} satisfies Meta<typeof UpdatePrompt>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Available: Story = {};

export const GitHubFormatting: Story = {
  args: {
    currentVersion: "0.0.1",
    releaseDate: "2026-08-21T05:24:02Z",
    releaseNotes: githubFormattingReleaseNotes,
    updateVersion: "0.1.0",
  },
  name: "GitHub Formatting",
};

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
