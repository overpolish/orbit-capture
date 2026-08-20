// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { updateDebug } from "./update-debug";

const REMIND_AFTER_KEY = "screenwide.updates.remindAfter";
const SKIPPED_VERSION_KEY = "screenwide.updates.skippedVersion";
const REMINDER_COOLDOWN_MS = 12 * 60 * 60 * 1000;

const read = (key: string) => {
  try {
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
};

const write = (key: string, value: string) => {
  try {
    window.localStorage.setItem(key, value);
  } catch {
    // A failed preference write should not prevent the user closing the prompt.
  }
};

export const scheduledUpdateCheckAt = () => {
  const stored = read(REMIND_AFTER_KEY);
  if (!stored) return null;
  const remindAfter = Number(stored);
  return Number.isFinite(remindAfter) ? remindAfter : null;
};

export const startupUpdateCheckDue = (now = Date.now()) => {
  const scheduled = scheduledUpdateCheckAt();
  const due = (scheduled ?? 0) <= now;
  updateDebug(
    due ? "Startup update check is due" : "Startup check is cooling down",
    { now, scheduled },
  );
  return due;
};

export const remindAboutUpdateLater = (now = Date.now()) => {
  const remindAfter = now + REMINDER_COOLDOWN_MS;
  write(REMIND_AFTER_KEY, String(remindAfter));
  updateDebug("Update reminder postponed for 12 hours", { remindAfter });
  return remindAfter;
};

export const skipUpdateVersion = (version: string) => {
  write(SKIPPED_VERSION_KEY, version);
  updateDebug("Update version skipped", { version });
};

export const updateVersionWasSkipped = (version: string) =>
  read(SKIPPED_VERSION_KEY) === version;
