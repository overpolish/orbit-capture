import { invoke } from "@tauri-apps/api/core";

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
