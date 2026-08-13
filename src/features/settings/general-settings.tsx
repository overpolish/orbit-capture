// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Folder, RotateCcw } from "lucide-react";

import { Button } from "../../components/base/button/button";
import { Checkbox } from "../../components/base/checkbox/checkbox";
import { PillGroup } from "../../components/base/pill-group/pill-group";

import { browseDefaultLocation } from "./api";
import { GeneralSettings } from "./types";

const folderName = (path: string | null) => {
  if (!path) return "System default";
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
};

function SettingRow({
  children,
  description,
  label,
}: {
  children: React.ReactNode;
  description: string;
  label: string;
}) {
  return (
    <div className="flex min-h-15 items-center gap-4 py-3">
      <div className="min-w-0 grow">
        <div className="text-sm font-medium">{label}</div>
        <div className="mt-0.5 text-xs text-muted">{description}</div>
      </div>
      <div className="shrink-0 whitespace-nowrap">{children}</div>
    </div>
  );
}

export function GeneralSettingsPanel({
  isSaving,
  onChange,
  settings,
}: {
  isSaving: boolean;
  onChange: (settings: GeneralSettings) => void;
  settings: GeneralSettings;
}) {
  const update = (changes: Partial<GeneralSettings>) => {
    onChange({ ...settings, ...changes });
  };
  const choose = (kind: "recording" | "screenshot") => {
    void browseDefaultLocation(kind).then((directory) => {
      if (!directory) return;
      update(
        kind === "recording"
          ? { recordingDirectory: directory }
          : { screenshotDirectory: directory },
      );
    });
  };

  return (
    <div className="divide-y divide-muted/15 px-4">
      <SettingRow
        description="The folder new recording exports start in. You can still change it per export."
        label="Default recording location"
      >
        <div className="flex items-center gap-1">
          <Button
            className="max-w-52 whitespace-nowrap"
            isDisabled={isSaving}
            onPress={() => {
              choose("recording");
            }}
            size="sm"
            variant="soft"
          >
            <Folder size={14} />
            <span className="truncate">
              {folderName(settings.recordingDirectory)}
            </span>
          </Button>
          {settings.recordingDirectory !== null ? (
            <Button
              aria-label="Use the system recording folder"
              className="max-w-52 whitespace-nowrap"
              icon
              isDisabled={isSaving}
              onPress={() => {
                update({ recordingDirectory: null });
              }}
              size="sm"
              variant="ghost"
            >
              <RotateCcw size={13} />
            </Button>
          ) : null}
        </div>
      </SettingRow>
      <SettingRow
        description="Used when a screenshot is sent to the export window."
        label="Default screenshot location"
      >
        <div className="flex items-center gap-1">
          <Button
            isDisabled={isSaving}
            onPress={() => {
              choose("screenshot");
            }}
            size="sm"
            variant="soft"
          >
            <Folder size={14} />
            <span className="truncate">
              {folderName(settings.screenshotDirectory)}
            </span>
          </Button>
          {settings.screenshotDirectory !== null ? (
            <Button
              aria-label="Use the system screenshot folder"
              icon
              isDisabled={isSaving}
              onPress={() => {
                update({ screenshotDirectory: null });
              }}
              size="sm"
              variant="ghost"
            >
              <RotateCcw size={13} />
            </Button>
          ) : null}
        </div>
      </SettingRow>
      <SettingRow
        description="Show the containing folder after a successful export."
        label="Open location after export"
      >
        <Checkbox
          isDisabled={isSaving}
          isSelected={settings.openLocationAfterExport}
          onChange={(openLocationAfterExport) => {
            update({ openLocationAfterExport });
          }}
          size="sm"
        />
      </SettingRow>
      <SettingRow
        description="Show camera, microphone and system audio activity while recording."
        label="Recording confidence checks"
      >
        <Checkbox
          isDisabled={isSaving}
          isSelected={settings.showRecordingConfidenceChecks}
          onChange={(showRecordingConfidenceChecks) => {
            update({ showRecordingConfidenceChecks });
          }}
          size="sm"
        />
      </SettingRow>
      <SettingRow
        description="Keep Orbit Capture's own windows, like the recording bar, in recordings and screenshots."
        label="Record Orbit Capture's windows"
      >
        <Checkbox
          isDisabled={isSaving}
          isSelected={settings.recordOrbitWindows}
          onChange={(recordOrbitWindows) => {
            update({ recordOrbitWindows });
          }}
          size="sm"
        />
      </SettingRow>
      <SettingRow
        description="Start Orbit Capture when you sign in."
        label="Launch at login"
      >
        <Checkbox
          isDisabled={isSaving}
          isSelected={settings.launchAtLogin}
          onChange={(launchAtLogin) => {
            update({ launchAtLogin });
          }}
          size="sm"
        />
      </SettingRow>
      <SettingRow
        description="Open the recording controls as soon as Orbit Capture starts."
        label="Show recording bar on launch"
      >
        <Checkbox
          isDisabled={isSaving}
          isSelected={settings.showRecordingBarOnLaunch}
          onChange={(showRecordingBarOnLaunch) => {
            update({ showRecordingBarOnLaunch });
          }}
          size="sm"
        />
      </SettingRow>
      <SettingRow
        description="Give yourself a moment before capture begins."
        label="Recording countdown"
      >
        <PillGroup
          ariaLabel="Recording countdown"
          display="label"
          isDisabled={isSaving}
          items={[
            { id: "0", label: "Off" },
            { id: "3", label: "3s" },
            { id: "5", label: "5s" },
          ]}
          onSelectionChange={(seconds) => {
            update({ recordingCountdownSeconds: Number(seconds) as 0 | 3 | 5 });
          }}
          selected={String(settings.recordingCountdownSeconds)}
        />
      </SettingRow>
    </div>
  );
}
