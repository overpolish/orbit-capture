import { PermissionSync } from "./features/permissions/permission-sync";
import { PermissionsWindow } from "./features/permissions/permissions-window";
import { RecordingBarWindow } from "./features/recording-controls/components/recording-bar-window";
import { RecordingInputSync } from "./features/recording-inputs/recording-input-sync";
import { RecordingOptionsWindow } from "./features/recording-inputs/recording-options-window";
import { RecordingSourceSelectorWindow } from "./features/recording-sources/recording-source-selector-window";
import { RecordingSourceSync } from "./features/recording-sources/recording-source-sync";
import { RegionSelectorWindow } from "./features/region-selector/region-selector-window";
import { StandaloneListboxSync } from "./features/standalone-listbox/standalone-listbox-sync";
import { StandaloneListboxWindow } from "./features/standalone-listbox/standalone-listbox-window";

export function App() {
  const content = (() => {
    switch (window.location.pathname) {
      case "/permissions":
        return <PermissionsWindow />;
      case "/recording-source-selector":
        return <RecordingSourceSelectorWindow />;
      case "/region-selector":
        return <RegionSelectorWindow />;
      case "/recording-options":
        return <RecordingOptionsWindow />;
      case "/standalone-listbox":
        return <StandaloneListboxWindow />;
      default:
        return <RecordingBarWindow />;
    }
  })();

  return (
    <>
      <PermissionSync />
      <RecordingInputSync />
      <RecordingSourceSync />
      <StandaloneListboxSync />
      {content}
    </>
  );
}
