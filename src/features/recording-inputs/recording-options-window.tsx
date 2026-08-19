// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useState } from "react";

import { openPermissionSettings, requestPermission } from "../permissions/api";
import { usePermissionStore } from "../permissions/store";
import { PermissionKind, PermissionStatus } from "../permissions/types";
import { hideStandaloneListbox } from "../standalone-listbox/api";
import { useStandaloneListboxStore } from "../standalone-listbox/store";

import {
  listCameras,
  listMicrophones,
  listSystemAudioSources,
} from "./devices-api";
import { RecordingOptions } from "./recording-options";
import { ALL_SYSTEM_AUDIO, useRecordingInputStore } from "./store";
import {
  CameraDevice,
  cameraRequestFps,
  InputDevice,
  SystemAudioSource,
} from "./types";
import { useAudioPreview } from "./use-audio-preview";
import { useCameraPreview } from "./use-camera-preview";

const grantPermission = (
  permission: PermissionKind,
  status: PermissionStatus,
) => {
  const action = status.canRequest
    ? requestPermission(permission)
    : openPermissionSettings(permission);
  void action;
};

const firstOrNull = <T,>(items: T[]): T | null =>
  items.length > 0 ? items[0] : null;

export function RecordingOptionsWindow() {
  const { hydrated, permissions } = usePermissionStore((state) => state);
  const cameraGranted = permissions.camera.granted;
  const microphoneGranted = permissions.microphone.granted;
  const screenRecordingGranted = permissions.screenRecording.granted;
  const [cameras, setCameras] = useState<CameraDevice[]>([]);
  const [microphones, setMicrophones] = useState<InputDevice[]>([]);
  const [audioSources, setAudioSources] = useState<SystemAudioSource[]>([
    ALL_SYSTEM_AUDIO,
  ]);
  const [isOpen, setIsOpen] = useState(false);
  const {
    cameraFlippedById,
    cameraPalById,
    fps,
    selectedCamera,
    selectedCameraMode,
    selectedMicrophone,
    selectedSystemAudio,
    setCameraFlipped,
    setCameraPal,
    setSelectedCameraMode,
    setSelectedCameraSelection,
    setSelectedMicrophone,
    setSelectedSystemAudio,
  } = useRecordingInputStore((state) => state);
  const cameraFlipped = selectedCamera
    ? (cameraFlippedById[selectedCamera.id] ?? false)
    : false;
  const cameraPal = selectedCamera
    ? (cameraPalById[selectedCamera.id] ?? false)
    : false;
  const previewsAllSystemAudio = selectedSystemAudio.some(
    (source) => source.id === ALL_SYSTEM_AUDIO.id,
  );
  const selectedApplicationIds = useMemo(
    () =>
      selectedSystemAudio
        .filter((source) => source.kind === "application")
        .map((source) => source.id),
    [selectedSystemAudio],
  );
  const selectedProcessIds = useMemo(
    () =>
      selectedSystemAudio
        .filter((source) => source.kind === "application")
        .flatMap((source) => source.processIds ?? []),
    [selectedSystemAudio],
  );
  const microphonePreview = useAudioPreview({
    active: isOpen && microphoneGranted && selectedMicrophone !== null,
    deviceId: selectedMicrophone?.id,
    kind: "microphone",
  });
  const systemAudioPreview = useAudioPreview({
    active:
      isOpen && (previewsAllSystemAudio || selectedApplicationIds.length > 0),
    applicationIds: previewsAllSystemAudio ? undefined : selectedApplicationIds,
    kind: "system",
    processIds: previewsAllSystemAudio ? undefined : selectedProcessIds,
  });
  const cameraPreview = useCameraPreview({
    active:
      isOpen &&
      cameraGranted &&
      selectedCamera !== null &&
      selectedCameraMode !== null,
    deviceId: selectedCamera?.id,
    mode: selectedCameraMode ?? undefined,
  });

  const refreshCameras = useCallback(async () => {
    // PAL is read from the store at call time rather than the closure: a camera
    // switch and a PAL toggle both refresh before this component re-renders.
    const requested = useRecordingInputStore.getState();
    const preferredFps = cameraRequestFps(
      fps,
      requested.selectedCamera
        ? (requested.cameraPalById[requested.selectedCamera.id] ?? false)
        : false,
    );
    const nextCameras = cameraGranted
      ? await listCameras(preferredFps).catch(() => [])
      : [];
    setCameras(nextCameras);
    const current = useRecordingInputStore.getState();
    const detectedCamera = nextCameras.find(
      (item) => item.id === current.selectedCamera?.id,
    );
    // Preserve a missing selection so the bar can warn instead of silently
    // switching the user's recording to a different device.
    const camera = current.selectedCamera
      ? (detectedCamera ?? current.selectedCamera)
      : (nextCameras.find((item) => item.isDefault) ??
        firstOrNull(nextCameras));
    const cameraIsMissing = current.selectedCamera && !detectedCamera;
    const mode =
      cameraIsMissing && current.selectedCameraMode
        ? current.selectedCameraMode
        : (camera?.modes.find(
            (item) => item.id === current.cameraModeIdById[camera.id],
          ) ??
          camera?.modes.find(
            (item) =>
              item.width === current.selectedCameraMode?.width &&
              item.height === current.selectedCameraMode.height,
          ) ??
          camera?.modes.find((item) => item.isDefault) ??
          firstOrNull(camera?.modes ?? []));
    setSelectedCameraSelection(camera, mode);
    return nextCameras;
  }, [cameraGranted, fps, setSelectedCameraSelection]);

  const selectCamera = useCallback(
    (camera: CameraDevice) => {
      const state = useRecordingInputStore.getState();
      const currentMode = state.selectedCameraMode;
      const rememberedModeId = state.cameraModeIdById[camera.id];
      const previousCamera = state.selectedCamera;
      const mode =
        camera.modes.find((item) => item.id === rememberedModeId) ??
        camera.modes.find(
          (item) =>
            item.width === currentMode?.width &&
            item.height === currentMode.height,
        ) ??
        camera.modes.find((item) => item.isDefault) ??
        firstOrNull(camera.modes);
      setSelectedCameraSelection(camera, mode);
      // The listed modes were enumerated for the previous camera's frame rate;
      // a camera whose PAL flag differs needs its modes fetched again.
      const previousPal = previousCamera
        ? (state.cameraPalById[previousCamera.id] ?? false)
        : false;
      if ((state.cameraPalById[camera.id] ?? false) !== previousPal)
        void refreshCameras();
    },
    [refreshCameras, setSelectedCameraSelection],
  );

  const refreshMicrophones = useCallback(async () => {
    const nextMicrophones = microphoneGranted
      ? await listMicrophones().catch(() => [])
      : [];
    setMicrophones(nextMicrophones);
    const current = useRecordingInputStore.getState();
    const detectedMicrophone = nextMicrophones.find(
      (item) => item.id === current.selectedMicrophone?.id,
    );
    const microphone = current.selectedMicrophone
      ? (detectedMicrophone ?? current.selectedMicrophone)
      : (nextMicrophones.find((item) => item.isDefault) ??
        firstOrNull(nextMicrophones));
    setSelectedMicrophone(microphone);
    return nextMicrophones;
  }, [microphoneGranted, setSelectedMicrophone]);

  const refreshAudioSources = useCallback(async () => {
    const applications = screenRecordingGranted
      ? await listSystemAudioSources().catch(() => [])
      : [];
    const nextAudioSources = [ALL_SYSTEM_AUDIO, ...applications];
    setAudioSources(nextAudioSources);
    const current = useRecordingInputStore.getState();
    const selectedAll = current.selectedSystemAudio.some(
      (source) => source.id === ALL_SYSTEM_AUDIO.id,
    );
    const systemAudio = selectedAll
      ? [ALL_SYSTEM_AUDIO]
      : current.selectedSystemAudio.map(
          (selected) =>
            nextAudioSources.find((source) => source.id === selected.id) ??
            selected,
        );

    setSelectedSystemAudio(
      systemAudio.length > 0 ? systemAudio : [ALL_SYSTEM_AUDIO],
    );
    return nextAudioSources;
  }, [screenRecordingGranted, setSelectedSystemAudio]);

  const refreshDevices = useCallback(async () => {
    await Promise.all([
      refreshCameras(),
      refreshMicrophones(),
      refreshAudioSources(),
    ]);
  }, [refreshAudioSources, refreshCameras, refreshMicrophones]);

  useEffect(() => {
    if (hydrated) void refreshDevices();
  }, [hydrated, refreshDevices]);

  useEffect(() => {
    let disposed = false;
    let unlistenOpened: (() => void) | undefined;
    let unlistenClosed: (() => void) | undefined;
    void Promise.all([
      listen("recording-options://opened", () => {
        setIsOpen(true);
      }),
      listen("recording-options://closed", () => {
        setIsOpen(false);
      }),
    ]).then(([opened, closed]) => {
      if (disposed) {
        opened();
        closed();
      } else {
        unlistenOpened = opened;
        unlistenClosed = closed;
      }
    });

    return () => {
      disposed = true;
      unlistenOpened?.();
      unlistenClosed?.();
    };
  }, []);

  return (
    <div
      className="h-full"
      onPointerDown={() => {
        useStandaloneListboxStore.getState().close();
        void hideStandaloneListbox();
      }}
    >
      <RecordingOptions
        audioSources={audioSources}
        cameraFlipped={cameraFlipped}
        cameraLocked={!hydrated || !permissions.camera.granted}
        cameraPal={cameraPal}
        cameraPreviewActive={cameraPreview.hasFrame}
        cameraPreviewRef={cameraPreview.canvasRef}
        cameraPreviewStarting={cameraPreview.isStarting}
        cameras={cameras}
        microphoneDecibels={microphonePreview.decibels}
        microphoneLocked={!hydrated || !permissions.microphone.granted}
        microphonePeak={microphonePreview.peak}
        microphonePreviewEnabled={
          selectedMicrophone !== null && microphoneGranted
        }
        microphones={microphones}
        onCameraChange={selectCamera}
        onCameraFlippedChange={(flipped) => {
          if (selectedCamera) setCameraFlipped(selectedCamera.id, flipped);
        }}
        onCameraLockedPress={() => {
          grantPermission("camera", permissions.camera);
        }}
        onCameraOptionsOpen={refreshCameras}
        onCameraPalChange={(pal) => {
          if (!selectedCamera) return;
          setCameraPal(selectedCamera.id, pal);
          // Modes carry their frame rate, so the list - and with it the
          // selected mode and the preview - must be fetched again at 25/50.
          void refreshCameras();
        }}
        onCameraResolutionChange={setSelectedCameraMode}
        onMicrophoneChange={setSelectedMicrophone}
        onMicrophoneLockedPress={() => {
          grantPermission("microphone", permissions.microphone);
        }}
        onMicrophoneOptionsOpen={refreshMicrophones}
        onSystemAudioChange={setSelectedSystemAudio}
        onSystemAudioOptionsOpen={refreshAudioSources}
        selectedCamera={selectedCamera}
        selectedCameraResolution={selectedCameraMode}
        selectedMicrophone={selectedMicrophone}
        selectedSystemAudio={selectedSystemAudio}
        standalone
        systemAudioDecibels={systemAudioPreview.decibels}
        systemAudioPeak={systemAudioPreview.peak}
        systemAudioPreviewEnabled={
          previewsAllSystemAudio || selectedApplicationIds.length > 0
        }
      />
    </div>
  );
}
