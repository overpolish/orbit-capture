import { PermissionSync } from "./features/permissions/permission-sync";
import { PermissionsWindow } from "./features/permissions/permissions-window";

export function App() {
  const content =
    window.location.pathname === "/permissions" ? (
      <PermissionsWindow />
    ) : (
      <main className="grid min-h-screen place-content-center gap-2 bg-content p-8 text-center text-content-fg">
        <div className="text-5xl font-semibold tracking-[-0.045em]">
          Orbit Capture
        </div>
        <p className="m-0 text-muted">Recording controls are coming next.</p>
      </main>
    );

  return (
    <>
      <PermissionSync />
      {content}
    </>
  );
}
