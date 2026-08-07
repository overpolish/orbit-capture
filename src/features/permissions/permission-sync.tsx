import { isTauri } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { useEffect } from "react";

import { getPermissionSnapshot } from "./api";
import { usePermissionStore } from "./store";
import { PermissionSnapshot } from "./types";

const PERMISSIONS_CHANGED_EVENT = "permissions://changed";

export function PermissionSync() {
  const setPermissions = usePermissionStore((state) => state.setPermissions);

  useEffect(() => {
    if (!isTauri()) return;

    let disposed = false;
    let unlisten: UnlistenFn | undefined;

    const synchronize = async () => {
      unlisten = await listen<PermissionSnapshot>(
        PERMISSIONS_CHANGED_EVENT,
        ({ payload }) => {
          setPermissions(payload);
        },
      );

      if (disposed) {
        unlisten();
        return;
      }

      const snapshot = await getPermissionSnapshot();
      setPermissions(snapshot);
    };

    void synchronize();

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [setPermissions]);

  return null;
}
