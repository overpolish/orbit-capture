// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import type { ReactNode } from "react";

type RulerStoryStageProps = {
  children: ReactNode;
  fullHeight?: boolean;
  size?: "cursor" | "overlay";
};

/**
 * These chips are absolutely positioned cursor followers, so they need a
 * positioned, screenshot-toned surface to sit on.
 *
 * The guides draw with `overflow-visible`, so the fixed-size stage clips them
 * and the wrapper scrolls whatever the Storybook canvas is too small to show.
 *
 * The probes draw with `overflow-visible`, so the fixed-size stage clips them
 * and the wrapper scrolls whatever the Storybook canvas is too small to show.
 *
 * The overlay draws with `overflow-visible`, so the fixed-size stage clips it
 * and the wrapper scrolls whatever the Storybook canvas is too small to show.
 */
export function RulerStoryStage({
  children,
  fullHeight,
  size = "overlay",
}: RulerStoryStageProps) {
  if (fullHeight)
    return (
      <div className="relative h-screen w-full overflow-hidden bg-neutral-100">
        {children}
      </div>
    );

  const stageClassName =
    size === "cursor"
      ? "relative h-[220px] w-[360px] overflow-hidden rounded-md bg-neutral-100"
      : "relative h-[400px] w-[640px] overflow-hidden rounded-md bg-neutral-100";

  return (
    <div className="max-w-full overflow-auto">
      <div className={stageClassName}>{children}</div>
    </div>
  );
}
