// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ExportSync } from "./features/exports/export-sync";
import { ExportWindow } from "./features/exports/export-window";
import { PermissionSync } from "./features/permissions/permission-sync";
import { PermissionsWindow } from "./features/permissions/permissions-window";
import { RecordingBarWindow } from "./features/recording-controls/components/recording-bar-window";
import { RecordingDockWindow } from "./features/recording-controls/components/recording-dock-window";
import { RecordingStateSync } from "./features/recording-controls/recording-state-sync";
import { RecordingInputSync } from "./features/recording-inputs/recording-input-sync";
import { RecordingOptionsWindow } from "./features/recording-inputs/recording-options-window";
import { RecordingSourceSelectorWindow } from "./features/recording-sources/recording-source-selector-window";
import { RecordingSourceSync } from "./features/recording-sources/recording-source-sync";
import { RegionSelectorWindow } from "./features/region-selector/region-selector-window";
import { SettingsWindow } from "./features/settings/settings-window";
import { StandaloneListboxSync } from "./features/standalone-listbox/standalone-listbox-sync";
import { StandaloneListboxWindow } from "./features/standalone-listbox/standalone-listbox-window";
import { TextRecognitionWindow } from "./features/text-recognition/text-recognition-window";
import { UpdatePromptWindow } from "./features/updates/update-prompt-window";

export function App() {
  const content = (() => {
    switch (window.location.pathname) {
      case "/export":
        return <ExportWindow />;
      case "/permissions":
        return <PermissionsWindow />;
      case "/recording-dock":
        return <RecordingDockWindow />;
      case "/recording-source-selector":
        return <RecordingSourceSelectorWindow />;
      case "/region-selector":
        return <RegionSelectorWindow />;
      case "/recording-options":
        return <RecordingOptionsWindow />;
      case "/settings":
        return <SettingsWindow />;
      case "/standalone-listbox":
        return <StandaloneListboxWindow />;
      case "/text-recognition":
        return <TextRecognitionWindow />;
      case "/update":
        return <UpdatePromptWindow />;
      default:
        return <RecordingBarWindow />;
    }
  })();

  return (
    <>
      <ExportSync />
      <PermissionSync />
      <RecordingInputSync />
      <RecordingSourceSync />
      <RecordingStateSync />
      <StandaloneListboxSync />
      {content}
    </>
  );
}
