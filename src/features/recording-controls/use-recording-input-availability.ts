// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect, useMemo, useState } from "react";

import {
  listCameras,
  listMicrophones,
  listSystemAudioSources,
} from "../recording-inputs/devices-api";
import {
  CameraDevice,
  InputDevice,
  RecordingFps,
  SystemAudioSource,
} from "../recording-inputs/types";

const AVAILABILITY_REFRESH_MS = 4_000;

const selectedDeviceIsDetected = (
  selected: InputDevice,
  detected: InputDevice[],
) =>
  detected.some((device) => device.id === selected.id) ||
  (selected.id === "default" && detected.some((device) => device.isDefault));

type DetectionResult = {
  detected: boolean;
  key: string;
};

type DetectionState = {
  camera: DetectionResult | null;
  microphone: DetectionResult | null;
  systemAudio: DetectionResult | null;
};

const initialDetection: DetectionState = {
  camera: null,
  microphone: null,
  systemAudio: null,
};

export function useRecordingInputAvailability({
  active,
  cameraEnabled,
  cameraPermissionGranted,
  fps,
  microphoneEnabled,
  microphonePermissionGranted,
  screenRecordingPermissionGranted,
  selectedCamera,
  selectedMicrophone,
  selectedSystemAudio,
  systemAudioEnabled,
}: {
  active: boolean;
  cameraEnabled: boolean;
  cameraPermissionGranted: boolean;
  fps: RecordingFps;
  microphoneEnabled: boolean;
  microphonePermissionGranted: boolean;
  screenRecordingPermissionGranted: boolean;
  selectedCamera: CameraDevice | null;
  selectedMicrophone: InputDevice | null;
  selectedSystemAudio: SystemAudioSource[];
  systemAudioEnabled: boolean;
}) {
  const selectedApplications = useMemo(
    () => selectedSystemAudio.filter((source) => source.kind === "application"),
    [selectedSystemAudio],
  );
  const checkCamera =
    active &&
    cameraEnabled &&
    cameraPermissionGranted &&
    selectedCamera !== null;
  const checkMicrophone =
    active &&
    microphoneEnabled &&
    microphonePermissionGranted &&
    selectedMicrophone !== null;
  const checkSystemAudio =
    active &&
    systemAudioEnabled &&
    screenRecordingPermissionGranted &&
    selectedApplications.length > 0;
  const cameraKey = checkCamera ? `${selectedCamera.id}:${String(fps)}` : null;
  const microphoneKey = checkMicrophone ? selectedMicrophone.id : null;
  const systemAudioKey = checkSystemAudio
    ? selectedApplications
        .map((source) => source.id)
        .sort()
        .join("\n")
    : null;
  const [detected, setDetected] = useState<DetectionState>(initialDetection);

  useEffect(() => {
    let disposed = false;
    let refreshing = false;

    const refresh = async () => {
      if (refreshing) return;
      refreshing = true;
      const [cameras, microphones, applications] = await Promise.all([
        checkCamera ? listCameras(fps).catch(() => null) : null,
        checkMicrophone ? listMicrophones().catch(() => null) : null,
        checkSystemAudio ? listSystemAudioSources().catch(() => null) : null,
      ]);
      refreshing = false;
      if (disposed) return;

      setDetected({
        camera:
          cameras && selectedCamera && cameraKey
            ? {
                detected: selectedDeviceIsDetected(selectedCamera, cameras),
                key: cameraKey,
              }
            : null,
        microphone:
          microphones && selectedMicrophone && microphoneKey
            ? {
                detected: selectedDeviceIsDetected(
                  selectedMicrophone,
                  microphones,
                ),
                key: microphoneKey,
              }
            : null,
        systemAudio:
          applications && systemAudioKey
            ? {
                detected: selectedApplications.every((selected) =>
                  applications.some((source) => source.id === selected.id),
                ),
                key: systemAudioKey,
              }
            : null,
      });
    };

    if (checkCamera || checkMicrophone || checkSystemAudio) {
      void refresh();
      const interval = window.setInterval(() => {
        void refresh();
      }, AVAILABILITY_REFRESH_MS);
      return () => {
        disposed = true;
        window.clearInterval(interval);
      };
    }

    return () => {
      disposed = true;
    };
  }, [
    checkCamera,
    checkMicrophone,
    checkSystemAudio,
    cameraKey,
    fps,
    microphoneKey,
    selectedApplications,
    selectedCamera,
    selectedMicrophone,
    systemAudioKey,
  ]);

  return {
    cameraMissing:
      detected.camera?.key === cameraKey && !detected.camera.detected,
    microphoneMissing:
      detected.microphone?.key === microphoneKey &&
      !detected.microphone.detected,
    systemAudioMissing:
      detected.systemAudio?.key === systemAudioKey &&
      !detected.systemAudio.detected,
  };
}
