import { type Meta, type StoryObj } from "@storybook/react-vite";

import { ExportArtifact } from "../types";

import { ExportPanel } from "./export-panel";

const screenshot: ExportArtifact = {
  extension: "png",
  height: 2234,
  id: 1,
  kind: "screenshot",
  suggestedFileStem: "Orbit Capture 2026-08-08 at 14.32.05",
  width: 3456,
};

const meta = {
  args: {
    artifact: screenshot,
    directory: "/Users/dom/Desktop",
    fileStem: screenshot.suggestedFileStem,
  },
  component: ExportPanel,
  parameters: {
    layout: "centered",
  },
  title: "Export/Export Panel",
} satisfies Meta<typeof ExportPanel>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Saving: Story = {
  args: { isSaving: true },
};

/** A name that has been emptied cannot be saved. */
export const EmptyName: Story = {
  args: { fileStem: "" },
};

export const WithError: Story = {
  args: { error: "That file name cannot be used" },
};

/** A long path truncates rather than pushing the Choose button off the row. */
export const LongDestination: Story = {
  args: {
    directory:
      "/Users/dom/Library/Mobile Documents/com~apple~CloudDocs/Screenshots/2026/August",
  },
};

/** Cancelling clears the artifact, which is what the window then shows. */
export const NothingPending: Story = {
  args: { artifact: null, fileStem: "" },
};
