// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useRef, useState } from "react";

import { hideUpdatePrompt, showUpdatePrompt } from "./api";
import { updateDebug } from "./update-debug";
import {
  remindAboutUpdateLater,
  scheduledUpdateCheckAt,
  skipUpdateVersion,
  startupUpdateCheckDue,
  updateVersionWasSkipped,
} from "./update-preferences";
import { UpdatePrompt } from "./update-prompt";
import { useUpdate } from "./use-update";

const developmentPreviewEnabled =
  import.meta.env.DEV && import.meta.env.VITE_SCREENWIDE_UPDATE_PREVIEW === "1";

const developmentReleaseNotes =
  "<ul><li>Capture windows and regions more reliably.</li><li>Added smoother cursor movement to exported recordings.</li><li>Remembered the last selected microphone and camera.</li><li>Improved export performance for <strong>longer recordings</strong>.</li><li>Added clearer feedback while preparing an export.</li><li>Improved recording controls on smaller displays.</li><li>Fixed occasional blank frames at the start of recordings.</li><li>Fixed window capture when an application changes size.</li><li>Fixed keyboard shortcuts after waking the computer.</li><li>Updated translations and accessibility labels.</li></ul>";

function UpdatePromptDevelopmentPreview() {
  const [installing, setInstalling] = useState(false);

  useEffect(() => {
    void showUpdatePrompt();
  }, []);

  return (
    <UpdatePrompt
      currentVersion="0.1.0"
      downloadProgress={installing ? 0.62 : null}
      error={null}
      onInstall={() => {
        setInstalling(true);
      }}
      onRemindLater={() => void hideUpdatePrompt()}
      onSkipVersion={() => void hideUpdatePrompt()}
      releaseDate="2026-08-18T12:00:00Z"
      releaseNotes={developmentReleaseNotes}
      status={installing ? "downloading" : "available"}
      updateVersion="0.2.0"
    />
  );
}

function LiveUpdatePromptWindow() {
  const [checkOnLaunch] = useState(startupUpdateCheckDue);
  const reminderTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const shownVersionRef = useRef<string | null>(null);
  const {
    checkForUpdates,
    currentVersion,
    discardUpdate,
    downloadProgress,
    error,
    installUpdate,
    releaseDate,
    releaseNotes,
    status,
    updateVersion,
  } = useUpdate({ autoCheck: checkOnLaunch });
  const busy = status === "downloading";

  const scheduleUpdateCheck = useCallback(
    (at: number) => {
      if (reminderTimerRef.current) clearTimeout(reminderTimerRef.current);
      reminderTimerRef.current = setTimeout(
        () => {
          reminderTimerRef.current = null;
          updateDebug("Reminder cooldown elapsed; checking again");
          void checkForUpdates();
        },
        Math.max(0, at - Date.now()),
      );
      updateDebug("Scheduled next update check", { at });
    },
    [checkForUpdates],
  );

  const remindLater = useCallback(() => {
    if (busy) return;
    updateDebug("Update prompt dismissed with reminder");
    shownVersionRef.current = null;
    scheduleUpdateCheck(remindAboutUpdateLater());
    void hideUpdatePrompt();
  }, [busy, scheduleUpdateCheck]);

  const skipVersion = useCallback(() => {
    if (!updateVersion || busy) return;
    updateDebug("Update prompt dismissed by skipping version", {
      updateVersion,
    });
    skipUpdateVersion(updateVersion);
    void hideUpdatePrompt();
    void discardUpdate();
  }, [busy, discardUpdate, updateVersion]);

  useEffect(() => {
    const scheduled = scheduledUpdateCheckAt();
    if (!checkOnLaunch && scheduled) scheduleUpdateCheck(scheduled);
    return () => {
      if (reminderTimerRef.current) clearTimeout(reminderTimerRef.current);
    };
  }, [checkOnLaunch, scheduleUpdateCheck]);

  useEffect(() => {
    if (status !== "available" || !updateVersion) return;
    if (updateVersionWasSkipped(updateVersion)) {
      updateDebug("Suppressing prompt for skipped version", { updateVersion });
      void discardUpdate();
      return;
    }
    if (shownVersionRef.current === updateVersion) return;
    shownVersionRef.current = updateVersion;
    updateDebug("Showing update prompt", { updateVersion });
    void showUpdatePrompt();
  }, [discardUpdate, status, updateVersion]);

  useEffect(() => {
    const listener = getCurrentWindow().onCloseRequested((event) => {
      event.preventDefault();
      remindLater();
    });
    return () => {
      void listener.then((off) => {
        off();
      });
    };
  }, [remindLater]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") remindLater();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [remindLater]);

  return (
    <UpdatePrompt
      currentVersion={currentVersion}
      downloadProgress={downloadProgress}
      error={error}
      onInstall={() => void installUpdate()}
      onRemindLater={remindLater}
      onSkipVersion={skipVersion}
      releaseDate={releaseDate}
      releaseNotes={releaseNotes}
      status={status}
      updateVersion={updateVersion}
    />
  );
}

export function UpdatePromptWindow() {
  return developmentPreviewEnabled ? (
    <UpdatePromptDevelopmentPreview />
  ) : (
    <LiveUpdatePromptWindow />
  );
}
