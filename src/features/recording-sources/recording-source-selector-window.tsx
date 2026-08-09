// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { convertFileSrc } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { AppWindowMac, ChevronDown, Monitor, SquareDashed } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useCallback, useEffect, useState } from "react";

import { Button } from "../../components/base/button/button";

import {
  collapseRecordingSourceSelector,
  listMonitors,
  listWindows,
  showRegionSelector,
  toggleRecordingSourceSelector,
} from "./api";
import { MonitorSelector } from "./monitor-selector";
import { useRecordingSourceStore } from "./store";
import { MonitorDetails, SelectorPlacement, WindowDetails } from "./types";
import { WindowSelector } from "./window-selector";
import { WindowUtilities } from "./window-utilities";

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
  const [windows, setWindows] = useState<WindowDetails[]>([]);
  const [windowsError, setWindowsError] = useState<string | null>(null);
  const [windowsLoading, setWindowsLoading] = useState(false);
  const {
    recordingMode,
    selectedMonitor,
    selectedWindow,
    setRegionEditing,
    setSelectedMonitor,
    setSelectedWindow,
  } = useRecordingSourceStore((state) => state);

  const refreshMonitors = useCallback(async () => {
    const available = await listMonitors();
    setMonitors(available);
    const { selectedMonitor, setSelectedMonitor } =
      useRecordingSourceStore.getState();
    const current = findCurrentMonitor(available, selectedMonitor);
    if (
      current &&
      JSON.stringify(current) !== JSON.stringify(selectedMonitor)
    ) {
      setSelectedMonitor(current);
    }
  }, []);

  const refreshWindows = useCallback(async () => {
    setWindowsLoading(true);
    setWindowsError(null);
    try {
      const available = await listWindows();
      setWindows(available);
      const { selectedWindow, setSelectedWindow } =
        useRecordingSourceStore.getState();
      if (
        selectedWindow &&
        !available.some((window) => window.id === selectedWindow.id)
      ) {
        setSelectedWindow(null);
      }
    } catch (error) {
      setWindowsError(
        error instanceof Error ? error.message : "Could not list windows",
      );
    } finally {
      setWindowsLoading(false);
    }
  }, []);

  useEffect(() => {
    void refreshMonitors();
  }, [refreshMonitors]);

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
              if (
                useRecordingSourceStore.getState().recordingMode === "window"
              ) {
                void refreshWindows();
              } else {
                void refreshMonitors();
              }
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
  }, [refreshMonitors, refreshWindows]);

  useEffect(() => {
    if (!isExpanded || recordingMode === "window") return;

    const interval = window.setInterval(() => {
      void refreshMonitors();
    }, REFRESH_INTERVAL_MS);

    return () => {
      window.clearInterval(interval);
    };
  }, [isExpanded, recordingMode, refreshMonitors]);

  return (
    <main className="window-surface fixed inset-0 flex overflow-hidden rounded-[10px] bg-content/92 p-2 text-content-fg">
      <section
        className={`flex h-full w-full flex-col gap-2 ${placement === "below" ? "justify-start" : "justify-end"}`}
      >
        {isExpanded ? (
          <div
            className={`flex min-h-0 grow flex-col gap-2 overflow-hidden ${placement === "below" ? "order-2" : "order-1"}`}
          >
            {recordingMode === "window" ? (
              <>
                <div className="min-h-0 grow overflow-hidden rounded-md">
                  <WindowSelector
                    error={windowsError}
                    isLoading={windowsLoading}
                    onSelect={setSelectedWindow}
                    selectedWindow={selectedWindow}
                    windows={windows}
                  />
                </div>
                <WindowUtilities selectedWindow={selectedWindow} />
              </>
            ) : (
              <div className="flex min-h-0 grow items-center justify-center overflow-hidden rounded-md inset-shadow-full">
                <MonitorSelector
                  monitors={monitors}
                  onSelect={(monitor) => {
                    setSelectedMonitor(monitor);
                    if (recordingMode === "region") {
                      void showRegionSelector(monitor);
                    }
                  }}
                  selectedMonitor={selectedMonitor}
                />
              </div>
            )}
          </div>
        ) : null}

        <div
          className={`flex h-6 w-full shrink-0 gap-2 ${placement === "below" ? "order-1" : "order-2"}`}
        >
          <Button
            className="h-full min-w-0 grow justify-center overflow-hidden"
            onPress={() => {
              void (isExpanded
                ? collapseRecordingSourceSelector()
                : toggleRecordingSourceSelector(recordingMode === "window"));
            }}
            showFocus={false}
            size="sm"
            variant="soft"
          >
            {recordingMode === "window" ? (
              selectedWindow?.appIconPath ? (
                <img
                  alt=""
                  className="size-4 shrink-0 object-contain"
                  src={convertFileSrc(selectedWindow.appIconPath)}
                />
              ) : (
                <AppWindowMac aria-hidden className="shrink-0" size={12} />
              )
            ) : (
              <Monitor aria-hidden className="shrink-0" size={12} />
            )}
            <span className="truncate">
              {recordingMode === "window"
                ? (selectedWindow?.title ?? "Choose a window")
                : (selectedMonitor?.name ?? "Choose a display")}
            </span>
            <ChevronDown
              aria-hidden
              className={`transform-gpu transition-transform duration-200 ${
                (isExpanded && placement === "below") ||
                (!isExpanded && placement === "above")
                  ? "rotate-180"
                  : "rotate-0"
              }`}
              size={12}
            />
          </Button>

          <AnimatePresence initial={false}>
            {recordingMode === "region" ? (
              <motion.div
                animate={{ opacity: 1, width: "auto" }}
                className="h-full overflow-hidden"
                exit={{ opacity: 0, width: 0 }}
                initial={{ opacity: 0, width: 0 }}
              >
                <Button
                  className="h-full"
                  onPress={() => {
                    void collapseRecordingSourceSelector();
                    setRegionEditing(true);
                  }}
                  showFocus={false}
                  size="sm"
                  variant="soft"
                >
                  <SquareDashed aria-hidden size={14} />
                  Edit
                </Button>
              </motion.div>
            ) : null}
          </AnimatePresence>
        </div>
      </section>
    </main>
  );
}
