// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, Update } from "@tauri-apps/plugin-updater";
import { useCallback, useEffect, useRef, useState } from "react";

import { fetchReleaseNotesHtml } from "./github-release";
import { updateDebug } from "./update-debug";

export type UpdateStatus =
  | "available"
  | "checking"
  | "development"
  | "downloading"
  | "error"
  | "idle"
  | "up-to-date";

const errorMessage = (reason: unknown) =>
  reason instanceof Error ? reason.message : String(reason);

export function useUpdate({ autoCheck = true } = {}) {
  const checkRequestRef = useRef(0);
  const updateRef = useRef<Update | null>(null);
  const [currentVersion, setCurrentVersion] = useState<string | null>(null);
  const [downloadProgress, setDownloadProgress] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [releaseDate, setReleaseDate] = useState<string | null>(null);
  const [releaseNotes, setReleaseNotes] = useState<string | null>(null);
  const [status, setStatus] = useState<UpdateStatus>(
    autoCheck ? "checking" : "idle",
  );
  const [updateVersion, setUpdateVersion] = useState<string | null>(null);

  const checkForUpdates = useCallback(async () => {
    const request = ++checkRequestRef.current;
    updateDebug("Starting update check", { request });
    setError(null);
    setStatus("checking");
    try {
      const [version, checksEnabled] = await Promise.all([
        getVersion(),
        invoke<boolean>("update_checks_enabled"),
      ]);
      if (request !== checkRequestRef.current) return;
      setCurrentVersion(version);

      if (!checksEnabled) {
        updateDebug("Skipping updater check for a development build", {
          currentVersion: version,
        });
        if (updateRef.current) await updateRef.current.close();
        updateRef.current = null;
        setDownloadProgress(null);
        setReleaseDate(null);
        setReleaseNotes(null);
        setUpdateVersion(null);
        setStatus("development");
        return;
      }

      const available = await check({ timeout: 30_000 });
      const notes = available
        ? await fetchReleaseNotesHtml(available.version)
        : null;
      if (request !== checkRequestRef.current) {
        if (available) await available.close();
        return;
      }
      if (updateRef.current) await updateRef.current.close();
      updateRef.current = available;
      setDownloadProgress(null);
      setReleaseDate(available?.date ?? null);
      setReleaseNotes(notes);
      setUpdateVersion(available?.version ?? null);
      setStatus(available ? "available" : "up-to-date");
      updateDebug(
        available ? "Update available" : "No newer update available",
        {
          currentVersion: version,
          updateVersion: available?.version,
        },
      );
    } catch (reason: unknown) {
      if (request !== checkRequestRef.current) return;
      updateDebug("Update check failed", { error: errorMessage(reason) });
      setError(errorMessage(reason));
      setStatus("error");
    }
  }, []);

  const discardUpdate = useCallback(async () => {
    updateDebug("Discarding the pending update");
    checkRequestRef.current += 1;
    const update = updateRef.current;
    updateRef.current = null;
    if (update) await update.close();
    setStatus("idle");
  }, []);

  const installUpdate = useCallback(async () => {
    const update = updateRef.current;
    if (!update) return;

    setDownloadProgress(null);
    setError(null);
    setStatus("downloading");
    updateDebug("Downloading and installing update", {
      updateVersion: update.version,
    });
    let downloaded = 0;
    let total: number | undefined;
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength;
          updateDebug("Update download started", { contentLength: total });
          setDownloadProgress(total ? 0 : null);
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          setDownloadProgress(total ? Math.min(downloaded / total, 1) : null);
        } else {
          setDownloadProgress(1);
          updateDebug("Update download finished");
        }
      });
      updateDebug("Update installed; relaunching Screenwide");
      await relaunch();
    } catch (reason: unknown) {
      updateDebug("Update installation failed", {
        error: errorMessage(reason),
      });
      setError(errorMessage(reason));
      setStatus("error");
    }
  }, []);

  useEffect(() => {
    if (autoCheck) void checkForUpdates();
    return () => {
      checkRequestRef.current += 1;
      const update = updateRef.current;
      updateRef.current = null;
      if (update) void update.close();
    };
  }, [autoCheck, checkForUpdates]);

  return {
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
  };
}
