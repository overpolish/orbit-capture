// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

export type TransformEdge = "bottom" | "left" | "right" | "top";

export const transformHandles: {
  edges: TransformEdge[];
  x: number;
  y: number;
}[] = [
  { edges: ["top", "left"], x: 0, y: 0 },
  { edges: ["top"], x: 0.5, y: 0 },
  { edges: ["top", "right"], x: 1, y: 0 },
  { edges: ["right"], x: 1, y: 0.5 },
  { edges: ["bottom", "right"], x: 1, y: 1 },
  { edges: ["bottom"], x: 0.5, y: 1 },
  { edges: ["bottom", "left"], x: 0, y: 1 },
  { edges: ["left"], x: 0, y: 0.5 },
];

export const cursorForTransformEdges = (edges: TransformEdge[]) => {
  const key = edges.join("-");
  if (key === "top" || key === "bottom") return "ns-resize";
  if (key === "left" || key === "right") return "ew-resize";
  if (key === "top-left" || key === "bottom-right") return "nwse-resize";
  return "nesw-resize";
};
