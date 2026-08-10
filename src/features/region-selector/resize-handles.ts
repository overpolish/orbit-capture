// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { CSSProperties } from "react";
import { HandleClasses, HandleStyles } from "react-rnd";

const HANDLE_STYLE: CSSProperties = {
  background: "var(--color-content)",
  border: "solid 1px white",
  borderRadius: "100%",
  height: 12,
  width: 12,
};

export const HANDLE_STYLES: HandleStyles = {
  bottom: {
    ...HANDLE_STYLE,
    cursor: "ns-resize",
    left: "50%",
    transform: "translateY(2px) translateX(-50%)",
  },
  bottomLeft: {
    ...HANDLE_STYLE,
    cursor: "nesw-resize",
    transform: "translateX(3px) translateY(-3px)",
  },
  bottomRight: {
    ...HANDLE_STYLE,
    cursor: "nwse-resize",
    transform: "translateX(-3px) translateY(-3px)",
  },
  left: {
    ...HANDLE_STYLE,
    cursor: "ew-resize",
    top: "50%",
    transform: "translateX(-2px) translateY(-50%)",
  },
  right: {
    ...HANDLE_STYLE,
    cursor: "ew-resize",
    top: "50%",
    transform: "translateX(2px) translateY(-50%)",
  },
  top: {
    ...HANDLE_STYLE,
    cursor: "ns-resize",
    left: "50%",
    transform: "translateY(-2px) translateX(-50%)",
  },
  topLeft: {
    ...HANDLE_STYLE,
    cursor: "nwse-resize",
    transform: "translateX(3px) translateY(3px)",
  },
  topRight: {
    ...HANDLE_STYLE,
    cursor: "nesw-resize",
    transform: "translateX(-3px) translateY(3px)",
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
