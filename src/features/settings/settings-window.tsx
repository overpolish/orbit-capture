// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { getCurrentWindow } from "@tauri-apps/api/window";
import { Info, Keyboard, Settings } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { OverflowShadow } from "../../components/base/overflow-shadow/overflow-shadow";
import { PillGroup } from "../../components/base/pill-group/pill-group";
import { WindowTitlebar } from "../../components/shared/window-titlebar/window-titlebar";
import { UpdatePanel } from "../updates/update-panel";

import {
  getGeneralSettings,
  getShortcutSettings,
  endShortcutCapture,
  hideSettings,
  setGeneralSettings,
  setShortcutBinding,
} from "./api";
import { GeneralSettingsPanel } from "./general-settings";
import { ShortcutField } from "./shortcut-field";
import { GeneralSettings, ShortcutAction, ShortcutSettings } from "./types";

const actions: {
  action: ShortcutAction;
  description: string;
  label: string;
}[] = [
  {
    action: "toggleRecordingBar",
    description: "Show or hide the recording controls.",
    label: "Show or hide recording bar",
  },
  {
    action: "startStopRecording",
    description: "Start with the current setup, or stop the active recording.",
    label: "Start or stop recording",
  },
  {
    action: "pauseResumeRecording",
    description: "Pause or resume the active recording.",
    label: "Pause or resume recording",
  },
  {
    action: "takeScreenshot",
    description: "Pick a region on screen, then capture it.",
    label: "Take screenshot",
  },
  {
    action: "recognizeText",
    description: "Draw around text anywhere on screen and copy it.",
    label: "Recognize text",
  },
  {
    action: "rulerOverlay",
    description: "Measure distances and align elements anywhere on screen.",
    label: "Ruler overlay",
  },
];

export function SettingsWindow() {
  const [section, setSection] = useState("general");
  const [general, setGeneral] = useState<GeneralSettings | null>(null);
  const [savingGeneral, setSavingGeneral] = useState(false);
  const [settings, setSettings] = useState<ShortcutSettings | null>(null);
  const [saving, setSaving] = useState<ShortcutAction | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([getGeneralSettings(), getShortcutSettings()])
      .then(([generalSettings, shortcutSettings]) => {
        setGeneral(generalSettings);
        setSettings(shortcutSettings);
      })
      .catch((reason: unknown) => {
        setError(String(reason));
      });
  }, []);

  const changeBinding = useCallback(
    (action: ShortcutAction, shortcut: string | null) => {
      setSaving(action);
      setError(null);
      setShortcutBinding(action, shortcut)
        .then(setSettings)
        .catch((reason: unknown) => {
          setError(String(reason));
        })
        .finally(() => {
          setSaving(null);
          void endShortcutCapture();
        });
    },
    [],
  );

  const changeGeneral = useCallback((next: GeneralSettings) => {
    setGeneral(next);
    setSavingGeneral(true);
    setError(null);
    setGeneralSettings(next)
      .then(setGeneral)
      .catch((reason: unknown) => {
        setError(String(reason));
        void getGeneralSettings().then(setGeneral);
      })
      .finally(() => {
        setSavingGeneral(false);
      });
  }, []);

  return (
    <main className="window-surface flex h-screen w-screen flex-col overflow-hidden rounded-[10px] bg-content/92 text-content-fg">
      <WindowTitlebar
        border={false}
        center={
          <PillGroup
            ariaLabel="Settings section"
            display="icon-label"
            items={[
              { icon: <Settings size={15} />, id: "general", label: "General" },
              { icon: <Keyboard size={15} />, id: "hotkeys", label: "Hotkeys" },
              { icon: <Info size={15} />, id: "about", label: "About" },
            ]}
            onSelectionChange={setSection}
            selected={section}
          />
        }
        onClose={() => void hideSettings()}
        onMinimize={() => void getCurrentWindow().minimize()}
      />
      <div className="flex min-h-0 grow flex-col">
        <header className="mx-auto w-full max-w-2xl shrink-0 px-6 pt-3 pb-4">
          <h1 className="text-lg font-semibold">
            {section === "general"
              ? "General"
              : section === "hotkeys"
                ? "Hotkeys"
                : "About"}
          </h1>
          <p className="mt-1 text-xs text-muted">
            {section === "general"
              ? "Defaults for capture, export and launch behaviour."
              : section === "hotkeys"
                ? "These work globally while Screenwide is running."
                : "Version information and software updates."}
          </p>
        </header>
        <section className="min-h-0 min-w-0 grow px-6 pb-6">
          <div className="mx-auto flex h-full max-w-2xl flex-col">
            {section === "general" && general ? (
              <OverflowShadow
                rootClassName="min-h-0 grow rounded-lg border border-muted/20 bg-neutral/15"
                shadowRadius="md"
              >
                <GeneralSettingsPanel
                  isSaving={savingGeneral}
                  onChange={changeGeneral}
                  settings={general}
                />
              </OverflowShadow>
            ) : null}
            {section === "hotkeys" ? (
              <OverflowShadow
                rootClassName="min-h-0 grow rounded-lg border border-muted/20 bg-neutral/15"
                shadowRadius="md"
              >
                <div className="divide-y divide-muted/15 px-4">
                  {actions.map(({ action, description, label }) => {
                    const binding = settings?.bindings.find(
                      (candidate) => candidate.action === action,
                    );
                    return (
                      <div
                        className="flex min-h-16 items-center gap-4 py-3"
                        key={action}
                      >
                        <div className="min-w-0 grow">
                          <div className="text-sm font-medium">{label}</div>
                          <div className="mt-0.5 text-xs text-muted">
                            {description}
                          </div>
                        </div>
                        <ShortcutField
                          isDisabled={!settings || saving !== null}
                          onChange={(shortcut) => {
                            changeBinding(action, shortcut);
                          }}
                          value={binding?.shortcut ?? null}
                        />
                      </div>
                    );
                  })}
                </div>
              </OverflowShadow>
            ) : null}
            {section === "about" ? (
              <OverflowShadow
                rootClassName="min-h-0 grow rounded-lg border border-muted/20 bg-neutral/15"
                shadowRadius="md"
              >
                <UpdatePanel />
              </OverflowShadow>
            ) : null}
            {error ? <p className="mt-3 text-xs text-error">{error}</p> : null}
          </div>
        </section>
      </div>
    </main>
  );
}
