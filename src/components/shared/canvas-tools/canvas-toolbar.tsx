// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ComponentProps } from "react";

import { cn } from "../../../lib/styling";

export function CanvasToolbar({
  children,
  className,
  ...props
}: ComponentProps<"div">) {
  return (
    <div
      className={cn(
        "flex items-center gap-1 rounded-md border border-muted/25 bg-content p-1 shadow-md",
        className,
      )}
      {...props}
    >
      {children}
    </div>
  );
}
