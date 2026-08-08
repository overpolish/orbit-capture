import { invoke } from "@tauri-apps/api/core";

import { ExportSnapshot } from "./types";

export const getExportSnapshot = () =>
  invoke<ExportSnapshot>("get_export_snapshot");

/**
 * Raw PNG bytes. The thumbnail by default; the full capture only when
 * something zooms in far enough to need the real pixels.
 */
export const getExportPreview = (full = false) =>
  invoke<ArrayBuffer>("get_export_preview", { full });

export const saveExport = (fileStem: string) =>
  invoke<string>("save_export", { fileStem });

export const copyExportToClipboard = async () => {
  await invoke<null>("copy_export_to_clipboard");
};

export const cancelExport = async () => {
  await invoke<null>("cancel_export");
};

export const browseExportDirectory = () =>
  invoke<string | null>("browse_export_directory");

export const setExportDirectory = async (directory: string) => {
  await invoke<null>("set_export_directory", { directory });
};
