// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ComponentProps, Ref } from "react";

import { cn } from "../../../lib/styling";

export type CanvasRect = {
  height: number;
  width: number;
  x: number;
  y: number;
};

export function SelectionFrame({
  bounds,
  children,
  className,
  ref,
  state = "selecting",
  ...props
}: Omit<ComponentProps<"div">, "style"> & {
  bounds: CanvasRect;
  ref?: Ref<HTMLDivElement>;
  state?: "loading" | "ready" | "selecting";
}) {
  return (
    <div
      className={cn(
        "absolute overflow-hidden rounded-sm before:pointer-events-none before:absolute before:inset-0 before:z-10 before:rounded-[inherit] before:border-2 before:border-dashed before:border-white before:content-['']",
        state === "loading" && "ocr-shine-border before:border-transparent",
        state === "ready" && "before:border-info",
        className,
      )}
      ref={ref}
      style={{
        height: bounds.height,
        left: bounds.x,
        top: bounds.y,
        width: bounds.width,
      }}
      {...props}
    >
      {children}
    </div>
  );
}
