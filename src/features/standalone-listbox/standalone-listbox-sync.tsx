import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { useEffect } from "react";

import {
  synchronizeStandaloneListboxStore,
  useStandaloneListboxStore,
} from "./store";

export function StandaloneListboxSync() {
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let disposed = false;

    // The open listbox is presentation state and must not survive an app launch.
    useStandaloneListboxStore.getState().close();
    window.addEventListener("storage", synchronizeStandaloneListboxStore);
    void listen("standalone-listbox://closed", () => {
      useStandaloneListboxStore.getState().close();
    }).then((listener) => {
      if (disposed) {
        listener();
      } else {
        unlisten = listener;
      }
    });

    return () => {
      disposed = true;
      unlisten?.();
      window.removeEventListener("storage", synchronizeStandaloneListboxStore);
    };
  }, []);

  return null;
}
