// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

import {
  CameraDevice,
  CameraResolution,
  InputDevice,
  RecordingFps,
  RecordingInputs,
  recordingFpsOptions,
  SystemAudioSource,
} from "./types";

const STORE_NAME = "screenwide-recording-inputs";

/** Smooth by default; halving it is an explicit choice to make a smaller file. */
const DEFAULT_FPS: RecordingFps = 60;

export const DEFAULT_CAMERA_MODE: CameraResolution = {
  fps: DEFAULT_FPS,
  height: 1080,
  id: "1920x1080@60",
  isDefault: true,
  label: "1920 × 1080",
  width: 1920,
};

export const DEFAULT_CAMERA: CameraDevice = {
  id: "default",
  isDefault: true,
  label: "Default camera",
  modes: [DEFAULT_CAMERA_MODE],
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

const isCameraResolution = (value: unknown): value is CameraResolution => {
  if (!value || typeof value !== "object") return false;
  const mode = value as Partial<CameraResolution>;
  return (
    typeof mode.id === "string" &&
    typeof mode.width === "number" &&
    typeof mode.height === "number" &&
    typeof mode.fps === "number"
  );
};

const isCameraDevice = (value: unknown): value is CameraDevice => {
  if (!value || typeof value !== "object") return false;
  const camera = value as Partial<CameraDevice>;
  return (
    typeof camera.id === "string" &&
    typeof camera.label === "string" &&
    Array.isArray(camera.modes) &&
    camera.modes.every(isCameraResolution)
  );
};

type RecordingInputStore = {
  cameraFlippedById: Record<string, boolean>;
  cameraModeIdById: Record<string, string>;
  cameraPalById: Record<string, boolean>;
  fps: RecordingFps;
  inputs: RecordingInputs;
  selectedCamera: CameraDevice | null;
  selectedCameraMode: CameraResolution | null;
  selectedMicrophone: InputDevice | null;
  selectedSystemAudio: SystemAudioSource[];
  setCameraFlipped: (cameraId: string, flipped: boolean) => void;
  setCameraPal: (cameraId: string, pal: boolean) => void;
  setFps: (fps: RecordingFps) => void;
  setInput: (input: keyof RecordingInputs, selected: boolean) => void;
  setSelectedCameraMode: (mode: CameraResolution | null) => void;
  setSelectedCameraSelection: (
    camera: CameraDevice | null,
    mode: CameraResolution | null,
  ) => void;
  setSelectedMicrophone: (microphone: InputDevice | null) => void;
  setSelectedSystemAudio: (sources: SystemAudioSource[]) => void;
};

export const useRecordingInputStore = create<RecordingInputStore>()(
  persist(
    (set) => ({
      cameraFlippedById: {},
      cameraModeIdById: {},
      cameraPalById: {},
      fps: DEFAULT_FPS,
      inputs: {
        camera: false,
        microphone: false,
        showCursor: true,
        systemAudio: false,
      },
      selectedCamera: null,
      selectedCameraMode: null,
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
      setCameraPal: (cameraId, pal) => {
        set((state) => ({
          cameraPalById: {
            ...state.cameraPalById,
            [cameraId]: pal,
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
      setSelectedCameraMode: (selectedCameraMode) => {
        set((state) => ({
          cameraModeIdById:
            state.selectedCamera && selectedCameraMode
              ? {
                  ...state.cameraModeIdById,
                  [state.selectedCamera.id]: selectedCameraMode.id,
                }
              : state.cameraModeIdById,
          selectedCameraMode,
        }));
      },
      setSelectedCameraSelection: (selectedCamera, selectedCameraMode) => {
        set((state) => ({
          cameraModeIdById:
            selectedCamera && selectedCameraMode
              ? {
                  ...state.cameraModeIdById,
                  [selectedCamera.id]: selectedCameraMode.id,
                }
              : state.cameraModeIdById,
          selectedCamera,
          selectedCameraMode,
        }));
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
          cameraModeIdById:
            persisted.cameraModeIdById &&
            typeof persisted.cameraModeIdById === "object"
              ? persisted.cameraModeIdById
              : {},
          cameraPalById:
            persisted.cameraPalById &&
            typeof persisted.cameraPalById === "object"
              ? persisted.cameraPalById
              : {},
          fps: recordingFpsOptions.includes(persisted.fps as RecordingFps)
            ? (persisted.fps as RecordingFps)
            : DEFAULT_FPS,
          // Older builds persisted only a physical camera. A formatless
          // camera cannot faithfully preview or record, so discovery chooses
          // a real mode instead of guessing behind the user's back.
          selectedCamera: isCameraDevice(persisted.selectedCamera)
            ? persisted.selectedCamera
            : null,
          selectedCameraMode: isCameraResolution(persisted.selectedCameraMode)
            ? persisted.selectedCameraMode
            : null,
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
