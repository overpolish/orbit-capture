import { invoke } from "@tauri-apps/api/core";

import { MonitorDetails } from "./types";

export const listMonitors = () => invoke<MonitorDetails[]>("list_monitors");

export const toggleRecordingSourceSelector = () =>
  invoke<null>("toggle_recording_source_selector");

export const collapseRecordingSourceSelector = () =>
  invoke<null>("collapse_recording_source_selector");

export const finishRecordingBarDrag = () =>
  invoke<null>("finish_recording_bar_drag");

export const hideRecordingUi = () => invoke<null>("hide_recording_ui");
