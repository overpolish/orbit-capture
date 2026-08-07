import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { ChevronDown, Monitor } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { Button } from "../../components/base/button/button";

import {
  collapseRecordingSourceSelector,
  listMonitors,
  toggleRecordingSourceSelector,
} from "./api";
import { MonitorSelector } from "./monitor-selector";
import { useRecordingSourceStore } from "./store";
import { MonitorDetails, SelectorPlacement } from "./types";

const REFRESH_INTERVAL_MS = 1_500;

const findCurrentMonitor = (
  monitors: MonitorDetails[],
  selected: MonitorDetails | null,
) => {
  if (selected) {
    const sameCaptureTarget = monitors.find(
      (monitor) => monitor.id === selected.id,
    );
    if (sameCaptureTarget) return sameCaptureTarget;

    const sameDisplay = monitors.find(
      (monitor) =>
        monitor.name === selected.name &&
        monitor.size.width === selected.size.width &&
        monitor.size.height === selected.size.height,
    );
    if (sameDisplay) return sameDisplay;
  }

  const primary = monitors.find((monitor) => monitor.isPrimary);
  if (primary) return primary;
  if (monitors.length === 0) return null;
  return monitors[0];
};

export function RecordingSourceSelectorWindow() {
  const [isExpanded, setIsExpanded] = useState(false);
  const [monitors, setMonitors] = useState<MonitorDetails[]>([]);
  const [placement, setPlacement] = useState<SelectorPlacement>("above");
  const { selectedMonitor, setSelectedMonitor } = useRecordingSourceStore(
    (state) => state,
  );

  const refresh = useCallback(async () => {
    const available = await listMonitors();
    setMonitors(available);
    const { selectedMonitor, setSelectedMonitor } =
      useRecordingSourceStore.getState();
    const current = findCurrentMonitor(available, selectedMonitor);
    if (current && current.id !== selectedMonitor?.id) {
      setSelectedMonitor(current);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    let disposed = false;
    let unlistenOpened: UnlistenFn | undefined;
    let unlistenCollapsed: UnlistenFn | undefined;
    let unlistenPlacement: UnlistenFn | undefined;

    const initialize = async () => {
      [unlistenOpened, unlistenCollapsed, unlistenPlacement] =
        await Promise.all([
          listen<SelectorPlacement>(
            "recording-source-selector://expanded",
            ({ payload }) => {
              setPlacement(payload);
              setIsExpanded(true);
              void refresh();
            },
          ),
          listen("recording-source-selector://collapsed", () => {
            setIsExpanded(false);
          }),
          listen<SelectorPlacement>(
            "recording-source-selector://placement",
            ({ payload }) => {
              setPlacement(payload);
            },
          ),
        ]);

      if (disposed) {
        unlistenOpened();
        unlistenCollapsed();
        unlistenPlacement();
      }
    };

    void initialize();

    return () => {
      disposed = true;
      unlistenOpened?.();
      unlistenCollapsed?.();
      unlistenPlacement?.();
    };
  }, [refresh]);

  useEffect(() => {
    if (!isExpanded) return;

    const interval = window.setInterval(() => {
      void refresh();
    }, REFRESH_INTERVAL_MS);

    return () => {
      window.clearInterval(interval);
    };
  }, [isExpanded, refresh]);

  return (
    <main className="fixed inset-0 flex overflow-hidden rounded-[10px] bg-content/92 p-2 text-content-fg">
      <section
        className={`flex h-full w-full flex-col gap-2 ${placement === "below" ? "justify-start" : "justify-end"}`}
      >
        {isExpanded ? (
          <div
            className={`min-h-0 grow overflow-hidden rounded-md inset-shadow-full ${placement === "below" ? "order-2" : "order-1"}`}
          >
            <div className="flex h-full items-center justify-center">
              <MonitorSelector
                monitors={monitors}
                onSelect={setSelectedMonitor}
                selectedMonitor={selectedMonitor}
              />
            </div>
          </div>
        ) : null}

        <Button
          className={`h-6 w-full min-w-0 justify-center overflow-hidden ${placement === "below" ? "order-1" : "order-2"}`}
          onPress={() => {
            void (isExpanded
              ? collapseRecordingSourceSelector()
              : toggleRecordingSourceSelector());
          }}
          showFocus={false}
          size="sm"
          variant="soft"
        >
          <Monitor aria-hidden className="shrink-0" size={12} />
          <span className="truncate">
            {selectedMonitor?.name ?? "Choose a display"}
          </span>
          <ChevronDown
            aria-hidden
            className={placement === "below" ? "rotate-180" : undefined}
            size={12}
          />
        </Button>
      </section>
    </main>
  );
}
