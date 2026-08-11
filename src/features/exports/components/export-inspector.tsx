// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  Camera,
  Cog,
  Mic,
  Monitor,
  MousePointer2,
  Volume2,
} from "lucide-react";
import { useState } from "react";

import { Checkbox } from "../../../components/base/checkbox/checkbox";
import { OverflowShadow } from "../../../components/base/overflow-shadow/overflow-shadow";
import { PillGroup } from "../../../components/base/pill-group/pill-group";
import { Slider } from "../../../components/base/slider/slider";
import {
  cameraResolutionScales,
  resolutionScales,
  scaledDimensions,
  scaledVideoDimensions,
} from "../resolution";
import {
  ExportArtifact,
  recordingAudioStreamIndex,
  recordingAudioTrackId,
  RecordingTrackId,
  RecordingVideoTrackId,
} from "../types";

import {
  RecordingSizeEstimate,
  VideoExportSettings,
} from "./recording-export-options";

type RecordingArtifact = Extract<ExportArtifact, { kind: "recording" }>;

const inspectorTabs = [
  { icon: <Cog size={15} />, id: "settings", label: "Settings" },
  { icon: <MousePointer2 size={15} />, id: "cursor", label: "Cursor" },
];

const trackTabs = (artifact: RecordingArtifact) => {
  const tabs: {
    icon: React.ReactNode;
    id: RecordingTrackId;
    label: string;
  }[] = [];
  if (artifact.primaryKind !== "audio") {
    const isCamera = artifact.primaryKind === "camera";
    tabs.push({
      icon: isCamera ? <Camera size={15} /> : <Monitor size={15} />,
      id: "primary",
      label: isCamera ? "Camera" : "Screen",
    });
  }
  if (artifact.camera) {
    tabs.push({
      icon: <Camera size={15} />,
      id: "camera",
      label: "Camera",
    });
  }
  tabs.push(
    ...artifact.audioTracks.map((track) => ({
      icon:
        track.kind === "microphone" ? <Mic size={15} /> : <Volume2 size={15} />,
      id: recordingAudioTrackId(track.streamIndex),
      label: track.label,
    })),
  );
  return tabs;
};

export function ExportInspector({
  artifact,
  bakeCamera,
  cameraCompression,
  cameraResolutionScalePercent,
  collapseAudio,
  compression,
  enabledAudioTrackCount = 0,
  enabledVideoTracks = [],
  error,
  estimatedSizeBytes,
  isEstimatingSize,
  isSaving,
  onBakeCameraChange,
  onCameraCompressionChange,
  onCameraResolutionScaleChange,
  onCollapseAudioChange,
  onCompressionChange,
  onResolutionScaleChange,
  onSelectedTrackChange,
  onSelectedTrackVolumeChange,
  resolutionScalePercent,
  selectedTrack,
  selectedTrackVolume = 0,
}: {
  artifact: RecordingArtifact;
  bakeCamera: boolean;
  cameraCompression: number;
  cameraResolutionScalePercent: number;
  compression: number;
  selectedTrack: RecordingTrackId | null;
  collapseAudio?: boolean;
  enabledAudioTrackCount?: number;
  enabledVideoTracks?: RecordingVideoTrackId[];
  error?: string | null;
  estimatedSizeBytes?: number | null;
  isEstimatingSize?: boolean;
  isSaving?: boolean;
  onBakeCameraChange?: (bake: boolean) => void;
  onCameraCompressionChange?: (compression: number) => void;
  onCameraResolutionScaleChange?: (scale: number) => void;
  onCollapseAudioChange?: (collapse: boolean) => void;
  onCompressionChange?: (compression: number) => void;
  onResolutionScaleChange?: (scale: number) => void;
  onSelectedTrackChange?: (trackId: RecordingTrackId) => void;
  onSelectedTrackVolumeChange?: (decibels: number) => void;
  resolutionScalePercent?: number;
  selectedTrackVolume?: number;
}) {
  const [inspectorTab, setInspectorTab] = useState("settings");
  const availableResolutionScales = resolutionScales(artifact);
  const effectiveResolutionScale =
    resolutionScalePercent ?? availableResolutionScales[0];
  const selectedAudioStreamIndex = recordingAudioStreamIndex(selectedTrack);
  const selectedAudioTrack = artifact.audioTracks.find(
    (track) => track.streamIndex === selectedAudioStreamIndex,
  );
  const videoSelection = new Set(enabledVideoTracks);
  const canBakeCamera =
    videoSelection.has("primary") && videoSelection.has("camera");
  const tabs = trackTabs(artifact);
  const effectiveSelectedTrack = selectedTrack ?? tabs[0].id;

  return (
    <aside className="flex min-h-0 min-w-0 flex-col border-r border-muted/15 bg-content/35">
      <OverflowShadow rootClassName="min-h-0 grow" shadowRadius="none">
        <div className="flex flex-col gap-4 p-4">
          <PillGroup
            ariaLabel="Inspector section"
            isDisabled={isSaving}
            items={inspectorTabs}
            onSelectionChange={setInspectorTab}
            selected={inspectorTab}
          />

          {inspectorTab === "settings" ? (
            <>
              {artifact.camera ? (
                <Checkbox
                  isDisabled={isSaving || !canBakeCamera}
                  isSelected={bakeCamera && canBakeCamera}
                  onChange={onBakeCameraChange}
                  size="sm"
                >
                  <span className="flex flex-col">
                    <span className="text-xs">Bake camera into recording</span>
                    <span className="text-xxs text-muted">
                      Position and crop it directly in the preview.
                    </span>
                  </span>
                </Checkbox>
              ) : null}

              {artifact.audioTracks.length > 1 ? (
                <Checkbox
                  isDisabled={isSaving || enabledAudioTrackCount < 2}
                  isSelected={collapseAudio}
                  onChange={onCollapseAudioChange}
                  size="sm"
                >
                  <span className="flex flex-col">
                    <span className="text-xs">Collapse audio tracks</span>
                    <span className="text-xxs text-muted">
                      Mix the selected tracks into one.
                    </span>
                  </span>
                </Checkbox>
              ) : null}

              <RecordingSizeEstimate
                estimatedSizeBytes={estimatedSizeBytes}
                isEstimatingSize={isEstimatingSize}
                originalSizeBytes={artifact.originalSizeBytes}
              />
            </>
          ) : null}

          {tabs.length > 0 ? (
            <div className="flex flex-col gap-3 border-t border-muted/15 pt-4">
              <PillGroup
                ariaLabel="Recording tracks"
                isDisabled={isSaving}
                items={tabs}
                onSelectionChange={(trackId) => {
                  onSelectedTrackChange?.(trackId as RecordingTrackId);
                }}
                selected={effectiveSelectedTrack}
              />

              {effectiveSelectedTrack === "primary" ? (
                <VideoExportSettings
                  compression={compression}
                  isDisabled={!artifact.canCompress || isSaving}
                  onCompressionChange={onCompressionChange}
                  onResolutionScaleChange={onResolutionScaleChange}
                  resolutionDimensions={(scale) =>
                    scaledDimensions(artifact, scale)
                  }
                  resolutionScale={effectiveResolutionScale}
                  resolutionScales={availableResolutionScales}
                />
              ) : null}

              {effectiveSelectedTrack === "camera" && artifact.camera ? (
                <VideoExportSettings
                  compression={cameraCompression}
                  isDisabled={!artifact.canCompress || isSaving}
                  onCompressionChange={onCameraCompressionChange}
                  onResolutionScaleChange={onCameraResolutionScaleChange}
                  resolutionDimensions={(scale) =>
                    scaledVideoDimensions({
                      height: artifact.camera?.height ?? 0,
                      scale,
                      sourceScale: 100,
                      width: artifact.camera?.width ?? 0,
                    })
                  }
                  resolutionScale={cameraResolutionScalePercent}
                  resolutionScales={cameraResolutionScales}
                />
              ) : null}

              {selectedAudioTrack ? (
                <Slider
                  aria-label={`${selectedAudioTrack.label} volume`}
                  isDisabled={isSaving}
                  label="Volume"
                  maxValue={12}
                  minValue={-60}
                  onChange={onSelectedTrackVolumeChange}
                  renderValue={(value) =>
                    value <= -60
                      ? "Muted"
                      : `${value > 0 ? "+" : ""}${value.toString()} dB`
                  }
                  step={1}
                  value={selectedTrackVolume}
                />
              ) : null}
            </div>
          ) : null}

          {error ? (
            <p className="m-0 text-xs text-error" role="alert">
              {error}
            </p>
          ) : null}
        </div>
      </OverflowShadow>
    </aside>
  );
}
