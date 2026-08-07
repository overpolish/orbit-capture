import { PermissionSync } from "./features/permissions/permission-sync";
import { PermissionsWindow } from "./features/permissions/permissions-window";
import { RecordingBarWindow } from "./features/recording-controls/components/recording-bar-window";

export function App() {
  const content =
    window.location.pathname === "/permissions" ? (
      <PermissionsWindow />
    ) : (
      <RecordingBarWindow />
    );

  return (
    <>
      <PermissionSync />
      {content}
    </>
  );
}
