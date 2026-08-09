import { invoke } from "@tauri-apps/api/core";

import { ExportSnapshot, RecordingPreview } from "./types";

export const getExportSnapshot = () =>
  invoke<ExportSnapshot>("get_export_snapshot");

/**
 * Raw PNG bytes. The thumbnail by default; the full capture only when
 * something zooms in far enough to need the real pixels.
 */
export const getExportPreview = (full = false) =>
  invoke<ArrayBuffer>("get_export_preview", { full });

export const getRecordingPreview = (artifactId: number) =>
  invoke<RecordingPreview>("get_recording_preview", { artifactId });

/**
 * The path of the one file the preview should play for these tracks.
 *
 * A video element plays a single audio track, so hearing two recorded tracks
 * at once means handing it a file in which they are already one. That is a
 * property of the player and not of the export, which keeps them separate.
 */
export const getRecordingPreviewMix = (
  artifactId: number,
  enabledStreamIndices: number[],
) =>
  invoke<string>("get_recording_preview_mix", {
    artifactId,
    enabledStreamIndices,
  });

type RecordingExportOptions = {
  artifactId: number;
  collapseAudio: boolean;
  compression: number;
  enabledStreamIndices: number[];
  resolutionScalePercent: number;
};

export const estimateRecordingExport = ({
  artifactId,
  collapseAudio,
  compression,
  enabledStreamIndices,
  resolutionScalePercent,
}: RecordingExportOptions) =>
  invoke<number>("estimate_recording_export", {
    artifactId,
    collapseAudio,
    compression,
    enabledStreamIndices,
    resolutionScalePercent,
  });

type SaveExportOptions = Omit<RecordingExportOptions, "artifactId"> & {
  fileStem: string;
};

export const saveExport = ({
  collapseAudio,
  compression,
  enabledStreamIndices,
  fileStem,
  resolutionScalePercent,
}: SaveExportOptions) =>
  invoke<string | null>("save_export", {
    collapseAudio,
    compression,
    enabledStreamIndices,
    fileStem,
    resolutionScalePercent,
  });

export const copyExportToClipboard = async () => {
  await invoke<null>("copy_export_to_clipboard");
};

export const cancelExport = async () => {
  await invoke<null>("cancel_export");
};

export const cancelExportJob = () => invoke<boolean>("cancel_export_job");

export const browseExportDirectory = () =>
  invoke<string | null>("browse_export_directory");

export const setExportDirectory = async (directory: string) => {
  await invoke<null>("set_export_directory", { directory });
};
