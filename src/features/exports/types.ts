/**
 * A capture waiting to be exported. Recordings become a second `kind` here, so
 * the window switches on it rather than assuming a screenshot.
 */
export type ExportArtifact = {
  extension: string;
  height: number;
  /** Unique per capture, so a replacement is never mistaken for the same one. */
  id: number;
  kind: "screenshot";
  suggestedFileStem: string;
  width: number;
};

export type ExportSnapshot = {
  artifact: ExportArtifact | null;
  directory: string | null;
};

export const initialExportSnapshot: ExportSnapshot = {
  artifact: null,
  directory: null,
};
