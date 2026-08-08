import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { useCallback, useEffect, useState } from "react";

import {
  browseExportDirectory,
  cancelExport,
  copyExportToClipboard,
  getExportPreview,
  saveExport,
  setExportDirectory,
} from "./api";
import { ExportPanel } from "./components/export-panel";
import { selectArtifact, selectDirectory, useExportStore } from "./store";

/** Matches the width the window is built with. */
const WINDOW_WIDTH = 460;
/** The root's `p-6` above and below the measured content. */
const WINDOW_PADDING = 48;

export function ExportWindow() {
  const artifact = useExportStore(selectArtifact);
  const directory = useExportStore(selectDirectory);
  const [fileStem, setFileStem] = useState("");
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);
  const [fullPreviewUrl, setFullPreviewUrl] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const suggestedFileStem = artifact?.suggestedFileStem ?? "";
  // Keyed on the capture rather than the object, so a replacement always
  // refetches - including the full-resolution copy, whose cached URL belongs to
  // the previous capture's pixels.
  const artifactId = artifact?.id;

  // A capture taken while the window is open replaces the pending one, so the
  // name follows the new suggestion rather than keeping the old capture's.
  useEffect(() => {
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setFileStem(suggestedFileStem);
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setError(null);
  }, [suggestedFileStem]);

  useEffect(() => {
    if (artifactId === undefined) return;

    let url: string | undefined;
    let disposed = false;

    void getExportPreview()
      .then((bytes) => {
        if (disposed) return;
        url = URL.createObjectURL(new Blob([bytes], { type: "image/png" }));
        setPreviewUrl(url);
      })
      .catch((cause: unknown) => {
        console.error("Could not load the export preview", cause);
      });

    return () => {
      disposed = true;
      if (url) URL.revokeObjectURL(url);
      setPreviewUrl(null);
      setFullPreviewUrl(null);
    };
  }, [artifactId]);

  // The full capture is only worth fetching once someone zooms past fit.
  useEffect(() => {
    if (!fullPreviewUrl) return;

    return () => {
      URL.revokeObjectURL(fullPreviewUrl);
    };
  }, [fullPreviewUrl]);

  // The window is sized to whatever the content actually measures, so the
  // spacing between sections stays even instead of one gap absorbing the slack
  // of a hand-picked window height.
  const onContentHeightChange = useCallback((height: number) => {
    if (!isTauri()) return;

    void getCurrentWindow().setSize(
      new LogicalSize(WINDOW_WIDTH, Math.ceil(height) + WINDOW_PADDING),
    );
  }, []);

  const report = (action: string) => (cause: unknown) => {
    console.error(`Could not ${action} the export`, cause);
    setError(cause instanceof Error ? cause.message : String(cause));
    setIsSaving(false);
  };

  return (
    <ExportPanel
      artifact={artifact}
      directory={directory}
      error={error}
      fileStem={fileStem}
      isSaving={isSaving}
      onBrowse={() => {
        browseExportDirectory()
          .then(async (chosen) => {
            if (chosen) await setExportDirectory(chosen);
          })
          .catch(report("choose a folder for"));
      }}
      onCancel={() => {
        cancelExport().catch(report("cancel"));
      }}
      onContentHeightChange={onContentHeightChange}
      onCopy={() => {
        copyExportToClipboard().catch(report("copy"));
      }}
      onFileStemChange={(value) => {
        setFileStem(value);
        setError(null);
      }}
      onNeedFullResolution={() => {
        if (fullPreviewUrl) return;

        getExportPreview(true)
          .then((bytes) => {
            setFullPreviewUrl(
              URL.createObjectURL(new Blob([bytes], { type: "image/png" })),
            );
          })
          .catch((cause: unknown) => {
            console.error("Could not load the full-resolution preview", cause);
          });
      }}
      onSave={() => {
        setIsSaving(true);
        setError(null);
        saveExport(fileStem)
          .then(() => {
            setIsSaving(false);
          })
          .catch(report("save"));
      }}
      previewUrl={fullPreviewUrl ?? previewUrl}
    />
  );
}
