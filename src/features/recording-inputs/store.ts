import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

import {
  InputDevice,
  RecordingFps,
  RecordingInputs,
  recordingFpsOptions,
  SystemAudioSource,
} from "./types";

const STORE_NAME = "orbit-capture-recording-inputs";

export const DEFAULT_CAMERA: InputDevice = {
  id: "default",
  isDefault: true,
  label: "Default camera",
};

export const DEFAULT_MICROPHONE: InputDevice = {
  id: "default",
  isDefault: true,
  label: "Default microphone",
};

/** Smooth by default; halving it is an explicit choice to make a smaller file. */
export const DEFAULT_FPS: RecordingFps = 60;

export const ALL_SYSTEM_AUDIO: SystemAudioSource = {
  id: "all",
  kind: "all",
  label: "All audio",
};

type RecordingInputStore = {
  cameraFlippedById: Record<string, boolean>;
  fps: RecordingFps;
  inputs: RecordingInputs;
  screenshotToClipboard: boolean;
  selectedCamera: InputDevice | null;
  selectedMicrophone: InputDevice | null;
  selectedSystemAudio: SystemAudioSource[];
  setCameraFlipped: (cameraId: string, flipped: boolean) => void;
  setFps: (fps: RecordingFps) => void;
  setInput: (input: keyof RecordingInputs, selected: boolean) => void;
  setScreenshotToClipboard: (toClipboard: boolean) => void;
  setSelectedCamera: (camera: InputDevice | null) => void;
  setSelectedMicrophone: (microphone: InputDevice | null) => void;
  setSelectedSystemAudio: (sources: SystemAudioSource[]) => void;
};

export const useRecordingInputStore = create<RecordingInputStore>()(
  persist(
    (set) => ({
      cameraFlippedById: {},
      fps: DEFAULT_FPS,
      inputs: {
        camera: false,
        microphone: false,
        showCursor: true,
        systemAudio: false,
      },
      screenshotToClipboard: true,
      selectedCamera: null,
      selectedMicrophone: null,
      selectedSystemAudio: [ALL_SYSTEM_AUDIO],
      setCameraFlipped: (cameraId, flipped) => {
        set((state) => ({
          cameraFlippedById: {
            ...state.cameraFlippedById,
            [cameraId]: flipped,
          },
        }));
      },
      setFps: (fps) => {
        set({ fps });
      },
      setInput: (input, selected) => {
        set((state) => ({
          inputs: { ...state.inputs, [input]: selected },
        }));
      },
      setScreenshotToClipboard: (screenshotToClipboard) => {
        set({ screenshotToClipboard });
      },
      setSelectedCamera: (selectedCamera) => {
        set({ selectedCamera });
      },
      setSelectedMicrophone: (selectedMicrophone) => {
        set({ selectedMicrophone });
      },
      setSelectedSystemAudio: (selectedSystemAudio) => {
        set({ selectedSystemAudio });
      },
    }),
    {
      merge: (persistedState, currentState) => {
        const persisted = persistedState as Partial<RecordingInputStore>;
        return {
          ...currentState,
          ...persisted,
          cameraFlippedById:
            persisted.cameraFlippedById &&
            typeof persisted.cameraFlippedById === "object"
              ? persisted.cameraFlippedById
              : {},
          fps: recordingFpsOptions.includes(persisted.fps as RecordingFps)
            ? (persisted.fps as RecordingFps)
            : DEFAULT_FPS,
          selectedSystemAudio: Array.isArray(persisted.selectedSystemAudio)
            ? persisted.selectedSystemAudio
            : [ALL_SYSTEM_AUDIO],
        };
      },
      name: STORE_NAME,
      storage: createJSONStorage(() => localStorage),
    },
  ),
);

export const synchronizeRecordingInputStore = (event: StorageEvent) => {
  if (event.key === STORE_NAME) {
    void useRecordingInputStore.persist.rehydrate();
  }
};
