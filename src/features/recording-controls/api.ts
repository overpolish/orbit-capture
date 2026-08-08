import { invoke } from "@tauri-apps/api/core";

import { RecordingSnapshot, StartRecordingOptions } from "./types";

export const getRecordingSnapshot = () =>
  invoke<RecordingSnapshot>("get_recording_snapshot");

// Every lifecycle call is awaited: Rust rejects illegal transitions, and a
// swallowed rejection would leave the UI claiming a state that does not exist.
export const startRecording = async (options: StartRecordingOptions) => {
  await invoke<null>("start_recording", { options });
};

export const pauseRecording = async () => {
  await invoke<null>("pause_recording");
};

export const resumeRecording = async () => {
  await invoke<null>("resume_recording");
};

export const stopRecording = async () => {
  await invoke<null>("stop_recording");
};

export const cancelRecording = async () => {
  await invoke<null>("cancel_recording");
};

// Committing on drag end mirrors the recording bar: the pill reappears where
// the user left it on the next recording.
export const finishRecordingDockDrag = () =>
  invoke<null>("finish_recording_dock_drag");
