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
import { InputDevice, SystemAudioSource } from "./types";
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
  const [cameras, setCameras] = useState<InputDevice[]>([]);
  const [microphones, setMicrophones] = useState<InputDevice[]>([]);
  const [audioSources, setAudioSources] = useState<SystemAudioSource[]>([
    ALL_SYSTEM_AUDIO,
  ]);
  const [isOpen, setIsOpen] = useState(false);
  const {
    cameraFlippedById,
    selectedCamera,
    selectedMicrophone,
    selectedSystemAudio,
    setCameraFlipped,
    setSelectedCamera,
    setSelectedMicrophone,
    setSelectedSystemAudio,
  } = useRecordingInputStore((state) => state);
  const cameraFlipped = selectedCamera
    ? (cameraFlippedById[selectedCamera.id] ?? false)
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
    active: isOpen && cameraGranted && selectedCamera !== null,
    deviceId: selectedCamera?.id,
  });

  const refreshCameras = useCallback(async () => {
    const nextCameras = cameraGranted
      ? await listCameras().catch(() => [])
      : [];
    setCameras(nextCameras);
    const current = useRecordingInputStore.getState();
    const camera =
      nextCameras.find((item) => item.id === current.selectedCamera?.id) ??
      nextCameras.find((item) => item.isDefault) ??
      firstOrNull(nextCameras);
    setSelectedCamera(camera);
    return nextCameras;
  }, [cameraGranted, setSelectedCamera]);

  const refreshMicrophones = useCallback(async () => {
    const nextMicrophones = microphoneGranted
      ? await listMicrophones().catch(() => [])
      : [];
    setMicrophones(nextMicrophones);
    const current = useRecordingInputStore.getState();
    const microphone =
      nextMicrophones.find(
        (item) => item.id === current.selectedMicrophone?.id,
      ) ??
      nextMicrophones.find((item) => item.isDefault) ??
      firstOrNull(nextMicrophones);
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
      : nextAudioSources.filter((source) =>
          current.selectedSystemAudio.some(
            (selected) => selected.id === source.id,
          ),
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
        cameraPreviewActive={cameraPreview.hasFrame}
        cameraPreviewRef={cameraPreview.canvasRef}
        cameras={cameras}
        microphoneDecibels={microphonePreview.decibels}
        microphoneLocked={!hydrated || !permissions.microphone.granted}
        microphonePeak={microphonePreview.peak}
        microphonePreviewEnabled={
          selectedMicrophone !== null && microphoneGranted
        }
        microphones={microphones}
        onCameraChange={setSelectedCamera}
        onCameraFlippedChange={(flipped) => {
          if (selectedCamera) setCameraFlipped(selectedCamera.id, flipped);
        }}
        onCameraLockedPress={() => {
          grantPermission("camera", permissions.camera);
        }}
        onCameraOptionsOpen={refreshCameras}
        onMicrophoneChange={setSelectedMicrophone}
        onMicrophoneLockedPress={() => {
          grantPermission("microphone", permissions.microphone);
        }}
        onMicrophoneOptionsOpen={refreshMicrophones}
        onSystemAudioChange={setSelectedSystemAudio}
        onSystemAudioOptionsOpen={refreshAudioSources}
        selectedCamera={selectedCamera}
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
