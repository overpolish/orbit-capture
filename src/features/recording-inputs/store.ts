import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

import { InputDevice, RecordingInputs, SystemAudioSource } from "./types";

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

export const ALL_SYSTEM_AUDIO: SystemAudioSource = {
  id: "all",
  kind: "all",
  label: "All audio",
};

type RecordingInputStore = {
  cameraFlippedById: Record<string, boolean>;
  inputs: RecordingInputs;
  selectedCamera: InputDevice | null;
  selectedMicrophone: InputDevice | null;
  selectedSystemAudio: SystemAudioSource[];
  setCameraFlipped: (cameraId: string, flipped: boolean) => void;
  setInput: (input: keyof RecordingInputs, selected: boolean) => void;
  setSelectedCamera: (camera: InputDevice | null) => void;
  setSelectedMicrophone: (microphone: InputDevice | null) => void;
  setSelectedSystemAudio: (sources: SystemAudioSource[]) => void;
};

export const useRecordingInputStore = create<RecordingInputStore>()(
  persist(
    (set) => ({
      cameraFlippedById: {},
      inputs: {
        camera: false,
        microphone: false,
        showCursor: true,
        systemAudio: false,
      },
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
      setInput: (input, selected) => {
        set((state) => ({
          inputs: { ...state.inputs, [input]: selected },
        }));
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
