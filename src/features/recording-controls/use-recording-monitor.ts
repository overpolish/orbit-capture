// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Channel } from "@tauri-apps/api/core";
import { type RefObject, useEffect, useRef, useState } from "react";

import { startRecordingMonitor, stopRecordingMonitor } from "./api";

const SOURCES_EVENT = 0;
const SYSTEM_AUDIO_EVENT = 1;
const MICROPHONE_EVENT = 2;
const CAMERA_EVENT = 3;
const SYSTEM_AUDIO_FLAG = 1;
const MICROPHONE_FLAG = 2;
const CAMERA_FLAG = 4;
const SILENCE_DECIBELS = -60;

let nextSubscriptionId = Date.now() * 1_000;

export type RecordingMonitorSnapshot = {
  cameraCanvasRef: RefObject<HTMLCanvasElement | null>;
  hasCamera: boolean;
  hasCameraFrame: boolean;
  hasMicrophone: boolean;
  hasSystemAudio: boolean;
  microphoneDecibels: number;
  systemAudioDecibels: number;
};

export function useRecordingMonitor(enabled = true): RecordingMonitorSnapshot {
  const cameraCanvasRef = useRef<HTMLCanvasElement>(null);
  const latestCameraFrameRef = useRef<ImageData | null>(null);
  const [hasCamera, setHasCamera] = useState(false);
  const [hasCameraFrame, setHasCameraFrame] = useState(false);
  const [hasMicrophone, setHasMicrophone] = useState(false);
  const [hasSystemAudio, setHasSystemAudio] = useState(false);
  const [microphoneDecibels, setMicrophoneDecibels] =
    useState(SILENCE_DECIBELS);
  const [systemAudioDecibels, setSystemAudioDecibels] =
    useState(SILENCE_DECIBELS);

  useEffect(() => {
    if (!enabled) return;
    let disposed = false;
    let frameRequest = 0;
    const subscriptionId = ++nextSubscriptionId;
    const channel = new Channel<ArrayBuffer>();

    const renderCameraFrame = () => {
      const frame = latestCameraFrameRef.current;
      const canvas = cameraCanvasRef.current;
      if (frame && canvas) {
        if (canvas.width !== frame.width || canvas.height !== frame.height) {
          canvas.width = frame.width;
          canvas.height = frame.height;
        }
        canvas.getContext("2d")?.putImageData(frame, 0, 0);
        latestCameraFrameRef.current = null;
        setHasCameraFrame(true);
      }
      frameRequest = requestAnimationFrame(renderCameraFrame);
    };

    channel.onmessage = (payload) => {
      if (disposed) return;
      const bytes = new Uint8Array(payload);
      const event = bytes[0];
      if (event === SOURCES_EVENT) {
        const flags = bytes[1];
        const camera = (flags & CAMERA_FLAG) !== 0;
        setHasSystemAudio((flags & SYSTEM_AUDIO_FLAG) !== 0);
        setHasMicrophone((flags & MICROPHONE_FLAG) !== 0);
        setHasCamera(camera);
        setHasCameraFrame(false);
        latestCameraFrameRef.current = null;
        setMicrophoneDecibels(SILENCE_DECIBELS);
        setSystemAudioDecibels(SILENCE_DECIBELS);
        return;
      }
      if (event === SYSTEM_AUDIO_EVENT || event === MICROPHONE_EVENT) {
        if (bytes.byteLength < 5) return;
        const decibels = new DataView(
          payload,
          bytes.byteOffset + 1,
          4,
        ).getFloat32(0, true);
        if (event === SYSTEM_AUDIO_EVENT) setSystemAudioDecibels(decibels);
        else setMicrophoneDecibels(decibels);
        return;
      }
      if (event === CAMERA_EVENT && bytes.byteLength >= 5) {
        const header = new DataView(payload, bytes.byteOffset + 1, 4);
        const width = header.getUint16(0, true);
        const height = header.getUint16(2, true);
        const pixels = new Uint8ClampedArray(payload, bytes.byteOffset + 5);
        if (pixels.byteLength === width * height * 4) {
          latestCameraFrameRef.current = new ImageData(pixels, width, height);
        }
      }
    };

    frameRequest = requestAnimationFrame(renderCameraFrame);
    startRecordingMonitor(subscriptionId, channel).catch((error: unknown) => {
      console.error("Could not monitor the active recording", error);
    });

    return () => {
      disposed = true;
      cancelAnimationFrame(frameRequest);
      latestCameraFrameRef.current = null;
      void stopRecordingMonitor(subscriptionId);
    };
  }, [enabled]);

  return {
    cameraCanvasRef,
    hasCamera,
    hasCameraFrame,
    hasMicrophone,
    hasSystemAudio,
    microphoneDecibels,
    systemAudioDecibels,
  };
}
