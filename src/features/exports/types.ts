type ExportArtifactBase = {
  extension: string;
  height: number;
  /** Unique per capture, so a replacement is never mistaken for the same one. */
  id: number;
  suggestedFileStem: string;
  width: number;
};

export type AudioTrackKind = "microphone" | "system-audio" | "unknown";

export type RecordingAudioTrack = {
  kind: AudioTrackKind;
  label: string;
  streamIndex: number;
};

export type PreparedAudioTrack = {
  kind: AudioTrackKind;
  label: string;
  /**
   * Which recorded track this came from. What a mix is asked for by, and what
   * identifies the row on screen.
   */
  streamIndex: number;
  waveform: number[];
};

export type RecordingPreview = {
  artifactId: number;
  tracks: PreparedAudioTrack[];
};

/**
 * A capture waiting to be exported. The window switches on `kind` rather than
 * assuming a screenshot: a recording is a file that gets moved, not pixels
 * that get encoded, and almost nothing about handling the two is the same.
 */
export type ExportArtifact =
  | (ExportArtifactBase & {
      audioTracks: RecordingAudioTrack[];
      canCompress: boolean;
      /** Zero for a recording recovered from an earlier run, whose length is unknown. */
      durationMs: number;
      kind: "recording";
      originalSizeBytes: number;
      /** The working file, played through the asset protocol. */
      path: string;
      /** Captured pixels per logical display point, multiplied by 100. */
      sourceScalePercent: number;
    })
  | (ExportArtifactBase & { kind: "screenshot" });

export type ExportSnapshot = {
  artifact: ExportArtifact | null;
  directory: string | null;
};

export const initialExportSnapshot: ExportSnapshot = {
  artifact: null,
  directory: null,
};
