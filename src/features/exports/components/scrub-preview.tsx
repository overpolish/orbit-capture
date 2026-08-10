// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  CameraOverlaySettings,
  PreparedAudioTrack,
  RecordingPreviewLayout,
} from "../types";

import { NativeRecordingPreview } from "./native-recording-preview";

export type ScrubPreviewProps = {
  artifactId: number;
  durationMs: number;
  audioError?: string | null;
  audioTracks?: PreparedAudioTrack[];
  bakeCamera?: boolean;
  cameraOverlay?: CameraOverlaySettings;
  enabledStreamIndices?: number[];
  isPreparingAudio?: boolean;
  isPreparingPreview?: boolean;
  onCameraOverlayChange?: (settings: CameraOverlaySettings) => void;
  onEnabledTracksChange?: (streamIndices: number[]) => void;
  previewLayout?: RecordingPreviewLayout;
};

/** The native Rust player is the sole recording-preview architecture. */
export function ScrubPreview(props: ScrubPreviewProps) {
  return <NativeRecordingPreview {...props} />;
}
