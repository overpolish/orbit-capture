import { isTauri } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { useEffect } from "react";

import { getRecordingSnapshot } from "./api";
import { useRecordingStore } from "./store";
import { RecordingSnapshot } from "./types";

const RECORDING_STATE_EVENT = "recording://state";

export function RecordingStateSync() {
  const setSnapshot = useRecordingStore((state) => state.setSnapshot);

  useEffect(() => {
    if (!isTauri()) return;

    let disposed = false;
    let unlisten: UnlistenFn | undefined;

    const synchronize = async () => {
      unlisten = await listen<RecordingSnapshot>(
        RECORDING_STATE_EVENT,
        ({ payload }) => {
          setSnapshot(payload);
        },
      );

      if (disposed) {
        unlisten();
        return;
      }

      const snapshot = await getRecordingSnapshot();
      setSnapshot(snapshot);
    };

    void synchronize();

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [setSnapshot]);

  return null;
}
