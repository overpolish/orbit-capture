// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

/** `01:04:07`, or `04:07` for anything under an hour. */
export const formatDuration = (durationMs: number) => {
  const total = Math.max(0, Math.floor(durationMs / 1000));
  const seconds = String(total % 60).padStart(2, "0");
  const minutes = Math.floor(total / 60) % 60;
  const hours = Math.floor(total / 3600);

  return hours > 0
    ? `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${seconds}`
    : `${String(minutes).padStart(2, "0")}:${seconds}`;
};

/**
 * A deliberately coarse remaining-time phrase for a running export, e.g.
 * `About 2 min remaining`, `About 40 sec remaining`, or
 * `Less than 10 sec remaining`. Rounds hard so the estimate never implies
 * precision it does not have.
 */
export const formatEta = (seconds: number) => {
  let total = Math.max(0, Math.round(seconds));
  // Below a minute the estimate is too jittery to put a number on, and the
  // remaining wait is short enough that a single steady phrase reads better
  // than a second-by-second countdown.
  if (total < 60) return "Less than a minute remaining";

  const days = Math.floor(total / 86_400);
  total -= days * 86_400;
  let hours = Math.floor(total / 3_600);
  total -= hours * 3_600;
  let minutes = Math.round(total / 60);
  // Rounding can carry a component to its own ceiling (59.6s → 60 min); roll it
  // up so "1 hr 60 min" never prints.
  if (minutes === 60) {
    minutes = 0;
    hours += 1;
  }
  let allDays = days;
  if (hours === 24) {
    hours = 0;
    allDays += 1;
  }

  const parts: string[] = [];
  if (allDays > 0) parts.push(`${String(allDays)} day${allDays === 1 ? "" : "s"}`);
  if (hours > 0) parts.push(`${String(hours)} hr`);
  // Always show minutes unless a larger unit already carries the estimate on
  // its own (e.g. exactly "1 hr remaining").
  if (minutes > 0 || parts.length === 0) parts.push(`${String(minutes)} min`);
  return `About ${parts.join(" ")} remaining`;
};

export const formatBytes = (bytes: number) => {
  if (bytes <= 0) return "Unknown size";
  const units = ["B", "KB", "MB", "GB"];
  const order = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    units.length - 1,
  );
  const value = bytes / 1024 ** order;
  return `${value.toFixed(value >= 10 || order === 0 ? 0 : 1)} ${units[order]}`;
};
