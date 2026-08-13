// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { CSSProperties } from "react";
import { HandleClasses, HandleStyles } from "react-rnd";

// The visual language comes from the shared TransformControls marquee (an
// 8px solid white dot on a 2px dashed border); these are react-rnd's hit
// areas, drawing the same dot inside a larger invisible box so the grab
// target stays comfortable. The offsets below centre the dot exactly on the
// marquee border given re-resizable's default handle anchors (edges at -5px,
// corners at -10px).
const HANDLE_STYLE: CSSProperties = {
  background: "radial-gradient(circle, white 0 4px, transparent 4.5px)",
  filter: "drop-shadow(0 0 2px rgb(0 0 0 / 50%))",
  height: 16,
  width: 16,
};

export const HANDLE_STYLES: HandleStyles = {
  bottom: {
    ...HANDLE_STYLE,
    cursor: "ns-resize",
    left: "50%",
    transform: "translateY(2.5px) translateX(-50%)",
  },
  bottomLeft: {
    ...HANDLE_STYLE,
    cursor: "nesw-resize",
    transform: "translateX(2.5px) translateY(-2.5px)",
  },
  bottomRight: {
    ...HANDLE_STYLE,
    cursor: "nwse-resize",
    transform: "translateX(-2.5px) translateY(-2.5px)",
  },
  left: {
    ...HANDLE_STYLE,
    cursor: "ew-resize",
    top: "50%",
    transform: "translateX(-2.5px) translateY(-50%)",
  },
  right: {
    ...HANDLE_STYLE,
    cursor: "ew-resize",
    top: "50%",
    transform: "translateX(2.5px) translateY(-50%)",
  },
  top: {
    ...HANDLE_STYLE,
    cursor: "ns-resize",
    left: "50%",
    transform: "translateY(-2.5px) translateX(-50%)",
  },
  topLeft: {
    ...HANDLE_STYLE,
    cursor: "nwse-resize",
    transform: "translateX(2.5px) translateY(2.5px)",
  },
  topRight: {
    ...HANDLE_STYLE,
    cursor: "nesw-resize",
    transform: "translateX(-2.5px) translateY(2.5px)",
  },
};

export const HANDLE_CLASSES: HandleClasses = {
  bottom: "region-handle-bottom",
  bottomLeft: "region-handle-bottom-left",
  bottomRight: "region-handle-bottom-right",
  left: "region-handle-left",
  right: "region-handle-right",
  top: "region-handle-top",
  topLeft: "region-handle-top-left",
  topRight: "region-handle-top-right",
};
