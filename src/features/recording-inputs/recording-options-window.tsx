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
    inputs,
    selectedCamera,
    selectedMicrophone,
    selectedSystemAudio,
    setSelectedCamera,
    setSelectedMicrophone,
    setSelectedSystemAudio,
  } = useRecordingInputStore((state) => state);
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
  const microphonePreview = useAudioPreview({
    active:
      isOpen &&
      inputs.microphone &&
      microphoneGranted &&
      selectedMicrophone !== null,
    deviceId: selectedMicrophone?.id,
    kind: "microphone",
  });
  const systemAudioPreview = useAudioPreview({
    active:
      isOpen &&
      inputs.systemAudio &&
      (previewsAllSystemAudio || selectedApplicationIds.length > 0),
    applicationIds: previewsAllSystemAudio ? undefined : selectedApplicationIds,
    kind: "system",
  });
  const cameraPreview = useCameraPreview({
    active: isOpen && inputs.camera && cameraGranted && selectedCamera !== null,
    deviceId: selectedCamera?.id,
  });

  const refreshDevices = useCallback(async () => {
    const [cameraResult, microphoneResult, applicationResult] =
      await Promise.allSettled([
        cameraGranted ? listCameras() : Promise.resolve([]),
        microphoneGranted ? listMicrophones() : Promise.resolve([]),
        screenRecordingGranted ? listSystemAudioSources() : Promise.resolve([]),
      ]);
    const nextCameras =
      cameraResult.status === "fulfilled" ? cameraResult.value : [];
    const nextMicrophones =
      microphoneResult.status === "fulfilled" ? microphoneResult.value : [];
    const nextAudioSources = [
      ALL_SYSTEM_AUDIO,
      ...(applicationResult.status === "fulfilled"
        ? applicationResult.value
        : []),
    ];

    setCameras(nextCameras);
    setMicrophones(nextMicrophones);
    setAudioSources(nextAudioSources);

    const current = useRecordingInputStore.getState();
    const camera =
      nextCameras.find((item) => item.id === current.selectedCamera?.id) ??
      nextCameras.find((item) => item.isDefault) ??
      firstOrNull(nextCameras);
    const microphone =
      nextMicrophones.find(
        (item) => item.id === current.selectedMicrophone?.id,
      ) ??
      nextMicrophones.find((item) => item.isDefault) ??
      firstOrNull(nextMicrophones);
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

    setSelectedCamera(camera);
    setSelectedMicrophone(microphone);
    setSelectedSystemAudio(
      systemAudio.length > 0 ? systemAudio : [ALL_SYSTEM_AUDIO],
    );
  }, [
    cameraGranted,
    microphoneGranted,
    screenRecordingGranted,
    setSelectedCamera,
    setSelectedMicrophone,
    setSelectedSystemAudio,
  ]);

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
        void refreshDevices();
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
  }, [refreshDevices]);

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
        cameraEnabled={inputs.camera}
        cameraLocked={!hydrated || !permissions.camera.granted}
        cameraPreviewActive={cameraPreview.hasFrame}
        cameraPreviewRef={cameraPreview.canvasRef}
        cameras={cameras}
        microphoneDecibels={microphonePreview.decibels}
        microphoneEnabled={inputs.microphone}
        microphoneLocked={!hydrated || !permissions.microphone.granted}
        microphonePeak={microphonePreview.peak}
        microphones={microphones}
        onCameraChange={setSelectedCamera}
        onCameraLockedPress={() => {
          grantPermission("camera", permissions.camera);
        }}
        onMicrophoneChange={setSelectedMicrophone}
        onMicrophoneLockedPress={() => {
          grantPermission("microphone", permissions.microphone);
        }}
        onSystemAudioChange={setSelectedSystemAudio}
        selectedCamera={selectedCamera}
        selectedMicrophone={selectedMicrophone}
        selectedSystemAudio={selectedSystemAudio}
        standalone
        systemAudioDecibels={systemAudioPreview.decibels}
        systemAudioEnabled={inputs.systemAudio}
        systemAudioPeak={systemAudioPreview.peak}
      />
    </div>
  );
}
