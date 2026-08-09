// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ClipboardCopy, Folder } from "lucide-react";
import {
  Input,
  Label,
  TextField as AriaTextField,
} from "react-aria-components";

import { Button } from "../../../components/base/button/button";
import { inputFieldVariants } from "../../../components/base/input-fields/input-field";
import { ExportArtifact } from "../types";

export function ExportDestinationForm({
  artifact,
  directory,
  error,
  fileStem,
  isExportPreparationPending,
  isSaving,
  onBrowse,
  onCancel,
  onCopy,
  onFileStemChange,
  onSave,
}: {
  artifact: ExportArtifact | null;
  directory: string | null;
  fileStem: string;
  error?: string | null;
  isExportPreparationPending?: boolean;
  isSaving?: boolean;
  onBrowse?: () => void;
  onCancel?: () => void;
  onCopy?: () => void;
  onFileStemChange?: (fileStem: string) => void;
  onSave?: () => void;
}) {
  const styles = inputFieldVariants({ size: "md", variant: "solid" });
  const isRecording = artifact?.kind === "recording";
  const canSave =
    Boolean(artifact) &&
    fileStem.trim().length > 0 &&
    !isExportPreparationPending &&
    !isSaving;

  return (
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

        {/* A movie is not something the clipboard can hold. */}
        {isRecording ? null : (
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
        )}
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
  );
}
