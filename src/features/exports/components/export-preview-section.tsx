// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useState } from "react";

import { ExportArtifact, PreparedAudioTrack } from "../types";

import { PreviewViewport } from "./preview-viewport";
import { RecordingMetadata, ScrubPreview } from "./scrub-preview";

/**
 * The screenshot section. Sibling to `RecordingSection`, and the reason the
 * frame around them does not know what it is showing.
 */
export function ScreenshotSection({
  artifact,
  onNeedFullResolution,
  previewUrl,
}: {
  artifact: ExportArtifact;
  onNeedFullResolution?: () => void;
  previewUrl?: string | null;
}) {
  return (
    <div className="flex flex-col gap-2">
      <PreviewViewport
        alt="Screenshot preview"
        artifactId={artifact.id}
        naturalHeight={artifact.height}
        naturalWidth={artifact.width}
        onNeedFullResolution={onNeedFullResolution}
        previewUrl={previewUrl}
      />
      <p className="m-0 text-center text-xxs text-muted tabular-nums">
        {artifact.width} &times; {artifact.height}
      </p>
    </div>
  );
}

/**
 * The recording section: a preview you skim, with what the file is underneath.
 *
 * Framed exactly like the still beside it - no box, no border, just the
 * picture and its shadow - because they are the same kind of thing to the
 * person deciding whether to keep it.
 */
export function RecordingSection({
  artifact,
  isPreparingRecordingPreview,
  isRemixingRecordingPreview,
  onEnabledTracksChange,
  previewUrl,
  recordingMixUrl,
  recordingPreviewError,
  recordingPreviewTracks,
  videoUrl,
}: {
  artifact: Extract<ExportArtifact, { kind: "recording" }>;
  isPreparingRecordingPreview?: boolean;
  isRemixingRecordingPreview?: boolean;
  onEnabledTracksChange?: (streamIndices: number[]) => void;
  previewUrl?: string | null;
  recordingMixUrl?: string | null;
  recordingPreviewError?: string | null;
  recordingPreviewTracks?: PreparedAudioTrack[];
  videoUrl?: string | null;
}) {
  // A recovered recording is presented knowing none of this, so whatever the
  // file itself reports fills the gap.
  const [discovered, setDiscovered] = useState<RecordingMetadata | null>(null);
  const width = artifact.width || (discovered?.width ?? 0);
  const height = artifact.height || (discovered?.height ?? 0);

  return (
    <div className="flex flex-col gap-2">
      <ScrubPreview
        artifactId={artifact.id}
        audioError={recordingPreviewError}
        audioTracks={recordingPreviewTracks}
        durationMs={artifact.durationMs}
        isPreparingAudio={isPreparingRecordingPreview}
        isRemixing={isRemixingRecordingPreview}
        key={artifact.id}
        mixUrl={recordingMixUrl}
        onEnabledTracksChange={onEnabledTracksChange}
        onMetadata={setDiscovered}
        posterUrl={previewUrl}
        videoUrl={videoUrl}
      />
      <div className="flex items-center justify-center gap-2 text-xxs text-muted tabular-nums">
        {width > 0 && height > 0 ? (
          <span>
            {width} &times; {height}
          </span>
        ) : null}
      </div>
    </div>
  );
}
