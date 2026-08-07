import { PermissionSync } from "./features/permissions/permission-sync";
import { PermissionsWindow } from "./features/permissions/permissions-window";
import { RecordingBarWindow } from "./features/recording-controls/components/recording-bar-window";
import { RecordingSourceSelectorWindow } from "./features/recording-sources/recording-source-selector-window";
import { RecordingSourceSync } from "./features/recording-sources/recording-source-sync";
import { RegionSelectorWindow } from "./features/region-selector/region-selector-window";

export function App() {
  const content = (() => {
    switch (window.location.pathname) {
      case "/permissions":
        return <PermissionsWindow />;
      case "/recording-source-selector":
        return <RecordingSourceSelectorWindow />;
      case "/region-selector":
        return <RegionSelectorWindow />;
      default:
        return <RecordingBarWindow />;
    }
  })();

  return (
    <>
      <PermissionSync />
      <RecordingSourceSync />
      {content}
    </>
  );
}
