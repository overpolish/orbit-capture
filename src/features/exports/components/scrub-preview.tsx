// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ReactNode } from "react";

import { RecordingOutputSettings } from "../screenshot-output";
import {
  AudioTrackVolume,
  CameraOverlaySettings,
  CursorEffectSettings,
  PreparedAudioTrack,
  RecordingPreviewLayout,
  RecordingTrackId,
  RecordingVideoTrackId,
} from "../types";

import { NativeRecordingPreview } from "./native-recording-preview";

export type ScrubPreviewProps = {
  artifactId: number;
  durationMs: number;
  previewSourceDimensions: Partial<
    Record<RecordingVideoTrackId, { height: number; width: number }>
  >;
  audioError?: string | null;
  audioTrackVolumes?: AudioTrackVolume[];
  audioTracks?: PreparedAudioTrack[];
  bakeCamera?: boolean;
  cameraOverlay?: CameraOverlaySettings;
  cursorEffects?: CursorEffectSettings;
  enabledStreamIndices?: number[];
  enabledVideoTracks?: RecordingVideoTrackId[];
  hasCursorData?: boolean;
  inspector?: ReactNode;
  isPreparingAudio?: boolean;
  isPreparingPreview?: boolean;
  onCameraOverlayChange?: (settings: CameraOverlaySettings) => void;
  onEnabledTracksChange?: (streamIndices: number[]) => void;
  onEnabledVideoTracksChange?: (tracks: RecordingVideoTrackId[]) => void;
  onRecordingOutputChange?: (
    trackId: RecordingVideoTrackId,
    settings: RecordingOutputSettings[RecordingVideoTrackId],
  ) => void;
  onSelectedTrackChange?: (trackId: RecordingTrackId | null) => void;
  onVideoTrackOrderChange?: (tracks: RecordingVideoTrackId[]) => void;
  previewLayout?: RecordingPreviewLayout;
  previewOutputDimensions?: Partial<
    Record<RecordingVideoTrackId, { height: number; width: number }>
  >;
  recordingOutput?: RecordingOutputSettings;
  selectedTrack?: RecordingTrackId | null;
};

/** The native Rust player is the sole recording-preview architecture. */
export function ScrubPreview(props: ScrubPreviewProps) {
  return <NativeRecordingPreview {...props} />;
}
