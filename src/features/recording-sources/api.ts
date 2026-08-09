// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Channel, invoke } from "@tauri-apps/api/core";

import { MonitorDetails, WindowDetails } from "./types";

export const listMonitors = () => invoke<MonitorDetails[]>("list_monitors");

export const listWindows = () => invoke<WindowDetails[]>("list_windows");

const windowIdentity = (window: WindowDetails) => ({
  id: window.id,
  pid: window.pid,
  title: window.title,
});

export const resizeWindow = (
  window: WindowDetails,
  width: number,
  height: number,
) =>
  invoke<null>("resize_window", {
    ...windowIdentity(window),
    height,
    width,
  });

export const centerWindow = (window: WindowDetails) =>
  invoke<null>("center_window", windowIdentity(window));

export const makeWindowBorderless = (window: WindowDetails) =>
  invoke<null>("make_window_borderless", windowIdentity(window));

export const restoreWindowBorder = (window: WindowDetails) =>
  invoke<null>("restore_window_border", windowIdentity(window));

export const toggleRecordingSourceSelector = (windowSelector: boolean) =>
  invoke<null>("toggle_recording_source_selector", { windowSelector });

export const collapseRecordingSourceSelector = () =>
  invoke<null>("collapse_recording_source_selector");

export const finishRecordingBarDrag = () =>
  invoke<null>("finish_recording_bar_drag");

export const hideRecordingUi = () => invoke<null>("hide_recording_ui");

export const setRecordingSourceSelectorVisible = (visible: boolean) =>
  invoke<null>("set_recording_source_selector_visible", { visible });

export const showRegionSelector = (monitor: MonitorDetails) =>
  invoke<null>("show_region_selector", {
    position: monitor.physicalPosition,
    size: monitor.physicalSize,
  });

export const hideRegionSelector = () => invoke<null>("hide_region_selector");

export const setRegionSelectorPassthrough = (passthrough: boolean) =>
  invoke<null>("set_region_selector_passthrough", { passthrough });

export const setRegionSelectorOpacity = (opacity: number) =>
  invoke<null>("set_region_selector_opacity", { opacity });

export const setRecordingControlsOpacity = (opacity: number) =>
  invoke<null>("set_recording_controls_opacity", { opacity });

export const takeMonitorScreenshot = (
  monitorId: number,
  channel: Channel<ArrayBuffer>,
) => invoke<null>("take_monitor_screenshot", { channel, monitorId });
