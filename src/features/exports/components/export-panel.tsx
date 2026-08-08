import { ClipboardCopy, Folder, X } from "lucide-react";
import { useEffect, useRef } from "react";
import {
  Input,
  Label,
  TextField as AriaTextField,
} from "react-aria-components";

import logoUrl from "../../../assets/orbit-capture-mark.svg";
import { Button } from "../../../components/base/button/button";
import { inputFieldVariants } from "../../../components/base/input-fields/input-field";
import { ExportArtifact } from "../types";

import { PreviewViewport } from "./preview-viewport";

type ExportPanelProps = {
  artifact: ExportArtifact | null;
  directory: string | null;
  fileStem: string;
  error?: string | null;
  isSaving?: boolean;
  onBrowse?: () => void;
  onCancel?: () => void;
  onContentHeightChange?: (height: number) => void;
  onCopy?: () => void;
  onFileStemChange?: (fileStem: string) => void;
  onNeedFullResolution?: () => void;
  onSave?: () => void;
  previewUrl?: string | null;
};

/**
 * The screenshot section. A recording artifact adds a sibling to this switch
 * rather than touching the frame around it.
 */
function ScreenshotSection({
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

export function ExportPanel({
  artifact,
  directory,
  error,
  fileStem,
  isSaving,
  onBrowse,
  onCancel,
  onContentHeightChange,
  onCopy,
  onFileStemChange,
  onNeedFullResolution,
  onSave,
  previewUrl,
}: ExportPanelProps) {
  const styles = inputFieldVariants({ size: "md", variant: "solid" });
  const canSave = Boolean(artifact) && fileStem.trim().length > 0 && !isSaving;
  const contentRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const content = contentRef.current;
    if (!content || !onContentHeightChange) return;

    const observer = new ResizeObserver(() => {
      onContentHeightChange(content.getBoundingClientRect().height);
    });
    observer.observe(content);

    return () => {
      observer.disconnect();
    };
  }, [onContentHeightChange]);

  return (
    <main className="window-surface h-screen overflow-hidden rounded-[10px] bg-content/92 p-6 text-content-fg">
      <div className="flex flex-col gap-4" ref={contentRef}>
        <header
          className="-m-6 mb-0 flex shrink-0 cursor-grab items-center gap-3 p-6 pb-0"
          data-tauri-drag-region
        >
          <img
            alt="Orbit Capture"
            className="pointer-events-none size-6 shrink-0 brightness-0 dark:invert"
            draggable={false}
            src={logoUrl}
          />
          <h1 className="pointer-events-none m-0 animate-gradient bg-linear-to-r from-orange-400 to-orange-500 bg-clip-text bg-size-[300%] text-2xl font-bold text-transparent">
            Save screenshot
          </h1>

          <Button
            aria-label="Close"
            className="group ml-auto cursor-default"
            icon
            onPress={onCancel}
            showFocus={false}
            size="sm"
            variant="ghost"
          >
            <X
              className="origin-center transform-gpu backface-hidden text-muted will-change-transform transition-[color,transform] group-data-[hovered]:scale-110 group-data-[hovered]:text-content-fg"
              size={20}
            />
          </Button>
        </header>

        {artifact ? (
          <ScreenshotSection
            artifact={artifact}
            onNeedFullResolution={onNeedFullResolution}
            previewUrl={previewUrl}
          />
        ) : (
          <div className="flex h-[220px] items-center justify-center rounded-md border border-muted/20 text-sm text-muted">
            Nothing to export
          </div>
        )}

        <form
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            if (canSave) onSave?.();
          }}
        >
          <AriaTextField
            aria-label="File name"
            className={styles.base()}
            isDisabled={!artifact || isSaving}
            onChange={onFileStemChange}
            value={fileStem}
          >
            <Label className={styles.label()}>Name</Label>
            <div className={styles.field()}>
              <div className={styles.inputWrapper()}>
                <Input className={styles.input()} />
                <span className="shrink-0 text-xs text-muted">
                  .{artifact?.extension ?? "png"}
                </span>
              </div>
            </div>
          </AriaTextField>

          <div className="flex flex-col gap-1">
            <span className={styles.label()}>Where</span>
            <div className="flex items-center gap-2">
              <Folder className="shrink-0 text-muted" size={16} />
              <span
                className="min-w-0 grow truncate text-xs text-muted"
                title={directory ?? undefined}
              >
                {directory ?? "No folder chosen"}
              </span>
              <Button
                isDisabled={isSaving}
                onPress={onBrowse}
                showFocus={false}
                size="sm"
                variant="soft"
              >
                Choose
              </Button>
            </div>
          </div>

          {error ? (
            <p className="m-0 text-xs text-error" role="alert">
              {error}
            </p>
          ) : null}

          <div className="flex shrink-0 items-center gap-2">
            <Button
              className="mr-auto"
              isDisabled={isSaving}
              onPress={onCancel}
              showFocus={false}
              size="sm"
              variant="soft"
            >
              Cancel
            </Button>

            <Button
              isDisabled={!artifact || isSaving}
              onPress={onCopy}
              showFocus={false}
              size="sm"
              variant="ghost"
            >
              <ClipboardCopy size={16} />
              Copy instead
            </Button>
            <Button
              color="info"
              isDisabled={!canSave}
              size="sm"
              type="submit"
              variant="solid"
            >
              {isSaving ? "Saving" : "Save"}
            </Button>
          </div>
        </form>
      </div>
    </main>
  );
}
