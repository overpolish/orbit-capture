// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { CheckCircle2, Download, RefreshCw } from "lucide-react";

import { Button } from "../../components/base/button/button";

import { ReleaseNotes } from "./release-notes";
import { useUpdate } from "./use-update";

export function UpdatePanel() {
  const {
    checkForUpdates,
    currentVersion,
    downloadProgress,
    error,
    installUpdate,
    releaseNotes,
    status,
    updateVersion,
  } = useUpdate();

  const busy = status === "checking" || status === "downloading";

  return (
    <div className="divide-y divide-muted/15 px-4">
      <div className="flex min-h-20 items-center gap-4 py-4">
        <div className="min-w-0 grow">
          <div className="text-sm font-medium">Screenwide</div>
          <div className="mt-0.5 text-xs text-muted">
            {currentVersion ? `Version ${currentVersion}` : "Reading version…"}
          </div>
        </div>
        <Button
          isDisabled={busy || status === "development"}
          onPress={() => void checkForUpdates()}
          size="sm"
          variant="soft"
        >
          <RefreshCw
            className={status === "checking" ? "animate-spin" : ""}
            size={14}
          />
          {status === "checking"
            ? "Checking…"
            : status === "development"
              ? "Development build"
              : "Check for updates"}
        </Button>
      </div>

      <div className="py-4">
        {status === "up-to-date" ? (
          <div className="flex items-center gap-2 text-sm">
            <CheckCircle2 className="text-success" size={16} />
            You’re using the latest version.
          </div>
        ) : null}

        {status === "development" ? (
          <div className="text-sm text-muted">
            Update checks are available in installed release builds.
          </div>
        ) : null}

        {updateVersion &&
        (status === "available" || status === "downloading") ? (
          <div>
            <div className="flex items-center justify-between gap-4">
              <div>
                <div className="text-sm font-medium">
                  Version {updateVersion} is available
                </div>
                <div className="mt-0.5 text-xs text-muted">
                  Screenwide will restart after installing the update.
                </div>
              </div>
              <Button
                color="info"
                isDisabled={busy}
                onPress={() => void installUpdate()}
                size="sm"
              >
                <Download size={14} />
                {status === "downloading"
                  ? "Installing…"
                  : "Update and restart"}
              </Button>
            </div>
            {status === "downloading" ? (
              <div className="mt-3 h-1.5 overflow-hidden rounded-full bg-muted/15">
                <div
                  className={`h-full rounded-full bg-info transition-[width] ${
                    downloadProgress === null ? "w-1/3 animate-pulse" : ""
                  }`}
                  style={
                    downloadProgress === null
                      ? undefined
                      : {
                          width: `${String(Math.round(downloadProgress * 100))}%`,
                        }
                  }
                />
              </div>
            ) : null}
            {releaseNotes ? (
              <div className="mt-4">
                <div className="text-xs font-semibold text-muted">
                  What’s new
                </div>
                <div className="mt-1 max-h-48 overflow-auto">
                  <ReleaseNotes html={releaseNotes} />
                </div>
              </div>
            ) : null}
          </div>
        ) : null}

        {status === "error" ? (
          <div>
            <p className="text-sm font-medium text-error">
              Couldn’t check for updates
            </p>
            <p className="mt-1 text-xs text-muted">{error}</p>
          </div>
        ) : null}
      </div>
    </div>
  );
}
