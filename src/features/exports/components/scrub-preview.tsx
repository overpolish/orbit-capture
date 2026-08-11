// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ReactNode } from "react";

import {
  AudioTrackVolume,
  CameraOverlaySettings,
  PreparedAudioTrack,
  RecordingPreviewLayout,
  RecordingTrackId,
  RecordingVideoTrackId,
} from "../types";

import { NativeRecordingPreview } from "./native-recording-preview";

export type ScrubPreviewProps = {
  artifactId: number;
  durationMs: number;
  audioError?: string | null;
  audioTrackVolumes?: AudioTrackVolume[];
  audioTracks?: PreparedAudioTrack[];
  bakeCamera?: boolean;
  cameraOverlay?: CameraOverlaySettings;
  enabledStreamIndices?: number[];
  enabledVideoTracks?: RecordingVideoTrackId[];
  inspector?: ReactNode;
  isPreparingAudio?: boolean;
  isPreparingPreview?: boolean;
  onCameraOverlayChange?: (settings: CameraOverlaySettings) => void;
  onEnabledTracksChange?: (streamIndices: number[]) => void;
  onEnabledVideoTracksChange?: (tracks: RecordingVideoTrackId[]) => void;
  onSelectedTrackChange?: (trackId: RecordingTrackId) => void;
  previewLayout?: RecordingPreviewLayout;
  previewOutputDimensions?: Partial<
    Record<RecordingVideoTrackId, { height: number; width: number }>
  >;
  selectedTrack?: RecordingTrackId | null;
};

/** The native Rust player is the sole recording-preview architecture. */
export function ScrubPreview(props: ScrubPreviewProps) {
  return <NativeRecordingPreview {...props} />;
}
