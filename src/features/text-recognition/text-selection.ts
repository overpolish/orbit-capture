// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { TextRecognitionResult } from "./api";

export type TextPosition = { line: number; offset: number };
export type TextRange = { end: TextPosition; start: TextPosition };
export type Point = { x: number; y: number };
export type ScreenSelection = {
  height: number;
  width: number;
  x: number;
  y: number;
};

export const selectionFrom = (start: Point, end: Point): ScreenSelection => ({
  height: Math.abs(end.y - start.y),
  width: Math.abs(end.x - start.x),
  x: Math.min(start.x, end.x),
  y: Math.min(start.y, end.y),
});

export const textPositionAt = (
  result: TextRecognitionResult,
  point: Point,
): TextPosition | undefined => {
  if (result.lines.length === 0) return;
  let closestLine = 0;
  let closestDistance = Number.POSITIVE_INFINITY;
  result.lines.forEach((line, index) => {
    const bottom = line.bounds.y + line.bounds.height;
    const distance =
      point.y < line.bounds.y
        ? line.bounds.y - point.y
        : point.y > bottom
          ? point.y - bottom
          : 0;
    if (distance < closestDistance) {
      closestDistance = distance;
      closestLine = index;
    }
  });
  const line = result.lines[closestLine];
  const nearest = line.characters.reduce<
    (typeof line.characters)[number] | undefined
  >((current, candidate) => {
    if (
      point.x >= candidate.bounds.x &&
      point.x <= candidate.bounds.x + candidate.bounds.width
    ) {
      return candidate;
    }
    if (!current) return candidate;
    const center = candidate.bounds.x + candidate.bounds.width / 2;
    const currentCenter = current.bounds.x + current.bounds.width / 2;
    return Math.abs(point.x - center) < Math.abs(point.x - currentCenter)
      ? candidate
      : current;
  }, undefined);
  if (nearest) {
    return {
      line: closestLine,
      offset:
        point.x < nearest.bounds.x + nearest.bounds.width / 2
          ? nearest.start
          : nearest.end,
    };
  }
  const fraction = Math.max(
    0,
    Math.min(
      1,
      (point.x - line.bounds.x) / Math.max(line.bounds.width, 0.0001),
    ),
  );
  return {
    line: closestLine,
    offset: Math.round(fraction * line.text.length),
  };
};

export const orderedRange = (
  anchor: TextPosition,
  focus: TextPosition,
): TextRange => {
  const anchorFirst =
    anchor.line < focus.line ||
    (anchor.line === focus.line && anchor.offset <= focus.offset);
  return anchorFirst
    ? { end: focus, start: anchor }
    : { end: anchor, start: focus };
};

const lineOffsets = (range: TextRange, line: number, length: number) => {
  if (line < range.start.line || line > range.end.line) return;
  return {
    end: line === range.end.line ? range.end.offset : length,
    start: line === range.start.line ? range.start.offset : 0,
  };
};

export const selectionRects = (
  result: TextRecognitionResult,
  ranges: readonly TextRange[],
) =>
  result.lines.flatMap((line, lineIndex) =>
    ranges.flatMap((range) => {
      const offsets = lineOffsets(range, lineIndex, line.text.length);
      if (!offsets || offsets.start === offsets.end) return [];
      const characters = line.characters.filter(
        (character) =>
          character.end > offsets.start && character.start < offsets.end,
      );
      if (characters.length === 0) {
        const length = Math.max(line.text.length, 1);
        return [
          {
            height: line.bounds.height,
            width: line.bounds.width * ((offsets.end - offsets.start) / length),
            x: line.bounds.x + line.bounds.width * (offsets.start / length),
            y: line.bounds.y,
          },
        ];
      }
      const left = Math.min(...characters.map(({ bounds }) => bounds.x));
      const top = Math.min(...characters.map(({ bounds }) => bounds.y));
      const right = Math.max(
        ...characters.map(({ bounds }) => bounds.x + bounds.width),
      );
      const bottom = Math.max(
        ...characters.map(({ bounds }) => bounds.y + bounds.height),
      );
      return [{ height: bottom - top, width: right - left, x: left, y: top }];
    }),
  );

export const selectedText = (
  result: TextRecognitionResult,
  ranges: readonly TextRange[],
) =>
  result.lines
    .flatMap((line, lineIndex) =>
      ranges
        .map((range) => lineOffsets(range, lineIndex, line.text.length))
        .filter((offsets) => offsets !== undefined)
        .sort((a, b) => a.start - b.start)
        .reduce<{ end: number; start: number }[]>((merged, offsets) => {
          const previous = merged[merged.length - 1];
          if (merged.length > 0 && offsets.start <= previous.end) {
            previous.end = Math.max(previous.end, offsets.end);
          } else {
            merged.push({ ...offsets });
          }
          return merged;
        }, [])
        .map(({ end, start }) => line.text.slice(start, end)),
    )
    .join("\n");

export const withoutLineBreaks = (text: string) =>
  text.replace(/\s*\r?\n\s*/g, " ").trim();
