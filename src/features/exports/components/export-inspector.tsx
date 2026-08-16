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

import { Alert } from "../../../components/base/alert/alert";
import { Checkbox } from "../../../components/base/checkbox/checkbox";
import { OverflowShadow } from "../../../components/base/overflow-shadow/overflow-shadow";
import { PillGroup } from "../../../components/base/pill-group/pill-group";
import { Slider } from "../../../components/base/slider/slider";
import { resizeCameraOverlayCentered } from "../camera-overlay-geometry";
import { resolutionScales, scaledDimensions } from "../resolution";
import {
  RecordingOutputSettings,
  recordingVideoTrackOrder,
  resizeScreenshotOutputCentered,
  ScreenshotOutputSettings,
} from "../screenshot-output";
import {
  CameraOverlaySettings,
  ExportArtifact,
  CursorEffectSettings,
  recordingAudioStreamIndex,
  recordingAudioTrackId,
  RecordingTrackId,
  RecordingVideoTrackId,
} from "../types";

import { CameraTrackSettings } from "./camera-track-settings";
import { CursorEffectControls } from "./cursor-effect-controls";
import {
  RecordingSizeEstimate,
  VideoExportSettings,
} from "./recording-export-options";
import { ScreenshotOutputControls } from "./screenshot-inspector";

type RecordingArtifact = Extract<ExportArtifact, { kind: "recording" }>;

const inspectorTabs = [
  { icon: <Cog size={15} />, id: "settings", label: "Settings" },
  { icon: <MousePointer2 size={15} />, id: "cursor", label: "Cursor" },
];

const trackTabs = (
  artifact: RecordingArtifact,
  recordingOutput?: RecordingOutputSettings,
) => {
  const tabs: {
    icon: React.ReactNode;
    id: RecordingTrackId;
    label: string;
  }[] = [];
  const videoTabs: typeof tabs = [];
  if (artifact.primaryKind !== "audio") {
    const isCamera = artifact.primaryKind === "camera";
    videoTabs.push({
      icon: isCamera ? <Camera size={15} /> : <Monitor size={15} />,
      id: "primary",
      label: isCamera ? "Camera" : "Screen",
    });
  }
  if (artifact.camera) {
    videoTabs.push({
      icon: <Camera size={15} />,
      id: "camera",
      label: "Camera",
    });
  }
  const videoOrder = recordingOutput
    ? recordingVideoTrackOrder(recordingOutput)
    : (["camera", "primary"] as const);
  tabs.push(
    ...videoTabs.sort(
      (left, right) =>
        videoOrder.indexOf(left.id as RecordingVideoTrackId) -
        videoOrder.indexOf(right.id as RecordingVideoTrackId),
    ),
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
  cameraOverlay,
  cameraResolutionScalePercent,
  collapseAudio,
  compression,
  cursorEffects,
  enabledAudioTrackCount = 0,
  enabledVideoTracks = [],
  error,
  estimatedSizeBytes,
  isEstimatingSize,
  isSaving,
  onBakeCameraChange,
  onCameraCompressionChange,
  onCameraOverlayChange,
  onCameraResolutionScaleChange,
  onCollapseAudioChange,
  onCompressionChange,
  onCursorEffectsChange,
  onRecordingOutputChange,
  onResolutionScaleChange,
  onSelectedTrackChange,
  onSelectedTrackVolumeChange,
  recordingOutput,
  resolutionScalePercent,
  selectedTrack,
  selectedTrackVolume = 0,
}: {
  artifact: RecordingArtifact;
  bakeCamera: boolean;
  cameraCompression: number;
  cameraOverlay: CameraOverlaySettings;
  cameraResolutionScalePercent: number;
  compression: number;
  cursorEffects: CursorEffectSettings;
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
  onCameraOverlayChange?: (settings: CameraOverlaySettings) => void;
  onCameraResolutionScaleChange?: (scale: number) => void;
  onCollapseAudioChange?: (collapse: boolean) => void;
  onCompressionChange?: (compression: number) => void;
  onCursorEffectsChange?: (settings: CursorEffectSettings) => void;
  onRecordingOutputChange?: (
    trackId: RecordingVideoTrackId,
    settings: ScreenshotOutputSettings,
  ) => void;
  onResolutionScaleChange?: (scale: number) => void;
  onSelectedTrackChange?: (trackId: RecordingTrackId) => void;
  onSelectedTrackVolumeChange?: (decibels: number) => void;
  recordingOutput?: RecordingOutputSettings;
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
  const hasRecordingSettings =
    Boolean(artifact.camera) || artifact.audioTracks.length > 1;
  const tabs = trackTabs(artifact, recordingOutput);
  const effectiveSelectedTrack = selectedTrack ?? tabs[0].id;
  const resizePrimaryOutput = (width: number, height: number) => {
    if (!recordingOutput) return;
    const next = resizeScreenshotOutputCentered({
      height,
      settings: recordingOutput.primary,
      source: { height: artifact.height, width: artifact.width },
      width,
    });
    if (bakeCamera && canBakeCamera)
      onCameraOverlayChange?.(
        resizeCameraOverlayCentered(
          cameraOverlay,
          recordingOutput.primary,
          next,
        ),
      );
    onRecordingOutputChange?.("primary", next);
  };

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

              {!hasRecordingSettings ? (
                <Alert color="neutral" role="status" size="sm">
                  No additional options are available for this recording.
                </Alert>
              ) : null}

              <RecordingSizeEstimate
                estimatedSizeBytes={estimatedSizeBytes}
                isEstimatingSize={isEstimatingSize}
                originalSizeBytes={artifact.originalSizeBytes}
              />
            </>
          ) : null}

          {inspectorTab === "cursor" && artifact.hasCursorData ? (
            <CursorEffectControls
              isSaving={Boolean(isSaving)}
              onChange={onCursorEffectsChange}
              settings={cursorEffects}
            />
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
                <div className="flex flex-col gap-4">
                  <VideoExportSettings
                    compression={compression}
                    isDisabled={!artifact.canCompress || isSaving}
                    onCompressionChange={onCompressionChange}
                    onResolutionScaleChange={onResolutionScaleChange}
                    resolutionDimensions={(scale) =>
                      scaledDimensions(artifact, scale)
                    }
                    resolutionScale={effectiveResolutionScale}
                    resolutionScales={
                      recordingOutput ? [] : availableResolutionScales
                    }
                  />
                  {recordingOutput ? (
                    <ScreenshotOutputControls
                      className=""
                      isSaving={isSaving}
                      onChange={(settings) => {
                        onRecordingOutputChange?.("primary", settings);
                      }}
                      onDimensionsChange={(width, height) => {
                        resizePrimaryOutput(width, height);
                      }}
                      settings={recordingOutput.primary}
                      sourceHeight={artifact.height}
                      sourceWidth={artifact.width}
                    />
                  ) : null}
                </div>
              ) : null}

              {effectiveSelectedTrack === "camera" && artifact.camera ? (
                <CameraTrackSettings
                  artifact={artifact}
                  availableResolutionScales={availableResolutionScales}
                  baked={bakeCamera && canBakeCamera}
                  cameraCompression={cameraCompression}
                  cameraResolutionScalePercent={cameraResolutionScalePercent}
                  compression={compression}
                  effectiveResolutionScale={effectiveResolutionScale}
                  isSaving={isSaving}
                  onCameraCompressionChange={onCameraCompressionChange}
                  onCameraResolutionScaleChange={onCameraResolutionScaleChange}
                  onCompressionChange={onCompressionChange}
                  onRecordingOutputChange={onRecordingOutputChange}
                  onResolutionScaleChange={onResolutionScaleChange}
                  recordingOutput={recordingOutput}
                  resizePrimaryOutput={resizePrimaryOutput}
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
