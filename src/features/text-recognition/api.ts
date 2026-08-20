// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Channel, invoke } from "@tauri-apps/api/core";

export type TextRecognitionResult = {
  lines: {
    bounds: { height: number; width: number; x: number; y: number };
    characters: {
      bounds: { height: number; width: number; x: number; y: number };
      end: number;
      start: number;
    }[];
    confidence: number;
    text: string;
  }[];
  text: string;
};

export type CapturedTextRegion = {
  height: number;
  imagePng: number[];
  width: number;
};

export type TextRecognitionSnapshot = {
  height: number;
  width: number;
};

export const cancelTextRecognition = () =>
  invoke<null>("cancel_text_recognition");

export const captureTextRegion = (
  monitorId: number,
  region: {
    position: { x: number; y: number };
    size: { height: number; width: number };
  },
) =>
  invoke<CapturedTextRegion>("capture_text_region", {
    monitorId,
    region,
  });

export const getTextRecognitionSnapshot = (
  monitorId: number,
  channel: Channel<ArrayBuffer>,
) =>
  invoke<TextRecognitionSnapshot>("get_text_recognition_snapshot", {
    channel,
    monitorId,
  });

export const recognizeCapturedText = () =>
  invoke<TextRecognitionResult>("recognize_captured_text");

export const copyRecognizedText = (text: string) =>
  invoke<null>("copy_recognized_text", { text });
