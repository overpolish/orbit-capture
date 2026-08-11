// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ComponentProps, useState } from "react";

import {
  recordingAudioStreamIndex,
  recordingAudioTrackId,
  RecordingTrackId,
} from "../types";

import { ExportPanel } from "./export-panel";

export function AudioRecordingStoryPanel(
  args: ComponentProps<typeof ExportPanel>,
) {
  const [enabledTracks, setEnabledTracks] = useState([0, 1]);
  const [selectedTrack, setSelectedTrack] = useState<RecordingTrackId | null>(
    () => recordingAudioTrackId(0),
  );
  const [volumes, setVolumes] = useState<Record<number, number>>({});
  return (
    <ExportPanel
      {...args}
      audioTrackVolumes={Object.entries(volumes).map(
        ([streamIndex, decibels]) => ({
          decibels,
          streamIndex: Number(streamIndex),
        }),
      )}
      enabledAudioTrackCount={enabledTracks.length}
      enabledStreamIndices={enabledTracks}
      onEnabledTracksChange={setEnabledTracks}
      onSelectedTrackChange={setSelectedTrack}
      onSelectedTrackVolumeChange={(decibels) => {
        const streamIndex = recordingAudioStreamIndex(selectedTrack);
        if (streamIndex === null) return;
        setVolumes((current) => ({ ...current, [streamIndex]: decibels }));
      }}
      selectedTrack={selectedTrack}
    />
  );
}

export function RecordingStoryPanel(args: ComponentProps<typeof ExportPanel>) {
  const recording = args.artifact?.kind === "recording" ? args.artifact : null;
  const [bakeCamera, setBakeCamera] = useState(args.bakeCamera ?? false);
  const [cameraCompression, setCameraCompression] = useState(
    args.cameraCompression ?? 0,
  );
  const [cameraOverlay, setCameraOverlay] = useState(args.cameraOverlay);
  const [cameraResolution, setCameraResolution] = useState(
    args.cameraResolutionScalePercent ?? 100,
  );
  const [collapseAudio, setCollapseAudio] = useState(
    args.collapseAudio ?? false,
  );
  const [compression, setCompression] = useState(args.compression ?? 0);
  const [enabledAudio, setEnabledAudio] = useState(
    () =>
      args.enabledStreamIndices ??
      recording?.audioTracks.map((track) => track.streamIndex) ??
      [],
  );
  const [enabledVideo, setEnabledVideo] = useState(
    () => args.enabledVideoTracks ?? [],
  );
  const [resolution, setResolution] = useState(
    args.resolutionScalePercent ?? 100,
  );
  const [selectedTrack, setSelectedTrack] = useState<RecordingTrackId | null>(
    () =>
      args.selectedTrack ??
      (recording?.primaryKind === "audio"
        ? recording.audioTracks[0]
          ? recordingAudioTrackId(recording.audioTracks[0].streamIndex)
          : null
        : "primary"),
  );

  return (
    <ExportPanel
      {...args}
      bakeCamera={bakeCamera}
      cameraCompression={cameraCompression}
      cameraOverlay={cameraOverlay}
      cameraResolutionScalePercent={cameraResolution}
      collapseAudio={collapseAudio}
      compression={compression}
      enabledAudioTrackCount={enabledAudio.length}
      enabledStreamIndices={enabledAudio}
      enabledVideoTracks={enabledVideo}
      onBakeCameraChange={setBakeCamera}
      onCameraCompressionChange={setCameraCompression}
      onCameraOverlayChange={setCameraOverlay}
      onCameraResolutionScaleChange={setCameraResolution}
      onCollapseAudioChange={setCollapseAudio}
      onCompressionChange={setCompression}
      onEnabledTracksChange={setEnabledAudio}
      onEnabledVideoTracksChange={setEnabledVideo}
      onResolutionScaleChange={setResolution}
      onSelectedTrackChange={setSelectedTrack}
      resolutionScalePercent={resolution}
      selectedTrack={selectedTrack}
    />
  );
}
