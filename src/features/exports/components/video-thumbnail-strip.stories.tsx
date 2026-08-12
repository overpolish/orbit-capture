// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { VideoThumbnailStrip } from "./video-thumbnail-strip";

import type { Meta, StoryObj } from "@storybook/react-vite";

const frame = (index: number) => {
  const hue = 205 + index * 4;
  const backgroundHue = hue.toString();
  const panelHue = (hue + 18).toString();
  const accentHue = (hue + 145).toString();
  const accentX = (30 + index * 5).toString();
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 160 90"><rect width="160" height="90" fill="hsl(${backgroundHue} 25% 18%)"/><rect x="8" y="8" width="92" height="56" rx="3" fill="hsl(${backgroundHue} 38% 36%)"/><rect x="106" y="8" width="46" height="74" rx="3" fill="hsl(${panelHue} 30% 28%)"/><circle cx="${accentX}" cy="70" r="8" fill="hsl(${accentHue} 70% 58%)"/></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
};

const thumbnails = Array.from({ length: 16 }, (_, index) => ({
  id: `frame-${index.toString()}`,
  url: frame(index),
}));

const meta = {
  component: VideoThumbnailStrip,
  decorators: [
    (Story) => (
      <div
        className="relative h-8 overflow-hidden rounded bg-muted/8"
        style={{ width: "min(80vw, 720px)" }}
      >
        <Story />
      </div>
    ),
  ],
  parameters: { layout: "centered" },
  title: "Features/Video Thumbnail Strip",
} satisfies Meta<typeof VideoThumbnailStrip>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: { enabled: true, thumbnails },
};

export const Disabled: Story = {
  args: { enabled: false, thumbnails },
};

export const Loading: Story = {
  args: {
    enabled: true,
    thumbnails: thumbnails.map((thumbnail, index) => ({
      ...thumbnail,
      url: index < 6 ? thumbnail.url : null,
    })),
  },
};
