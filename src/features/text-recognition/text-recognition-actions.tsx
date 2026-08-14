// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Copy, Pilcrow, RotateCcw, Trash2, X } from "lucide-react";

import { Button } from "../../components/base/button/button";
import { CanvasToolbar } from "../../components/shared/canvas-tools/canvas-toolbar";
import { ConfirmActionButton } from "../../components/shared/confirm-action-button/confirm-action-button";
import { cn } from "../../lib/styling";

export function TextRecognitionCloseAction({
  isMac,
  onClose,
}: {
  isMac: boolean;
  onClose: () => void;
}) {
  return (
    <CanvasToolbar
      className={cn(
        "absolute left-1/2 -translate-x-1/2",
        isMac ? "top-12" : "top-2",
      )}
      onPointerDown={(event) => {
        event.stopPropagation();
      }}
    >
      <ConfirmActionButton
        armedIcon={<Trash2 className="text-error" size={18} />}
        armedLabel="Confirm closing text recognition"
        className="h-7 w-7"
        idleIcon={<X size={18} />}
        idleLabel="Close text recognition"
        onConfirm={onClose}
      />
    </CanvasToolbar>
  );
}

export function TextRecognitionActions({
  onClose,
  onCopyAll,
  onCopyAsParagraph,
  onReset,
}: {
  onClose: () => void;
  onCopyAll: () => void;
  onCopyAsParagraph: () => void;
  onReset: () => void;
}) {
  return (
    <>
      <Button
        className="shrink-0 whitespace-nowrap"
        onPress={onCopyAll}
        showFocus={false}
        size="sm"
        variant="ghost"
      >
        <Copy size={15} />
        Copy all
      </Button>
      <Button
        className="shrink-0 whitespace-nowrap"
        onPress={onCopyAsParagraph}
        showFocus={false}
        size="sm"
        variant="ghost"
      >
        <Pilcrow size={15} />
        Copy as paragraph
      </Button>
      <Button
        aria-label="Recognize another area"
        icon
        onPress={onReset}
        showFocus={false}
        size="sm"
        variant="ghost"
      >
        <RotateCcw size={15} />
      </Button>
      <ConfirmActionButton
        armedIcon={<Trash2 className="text-error" size={15} />}
        armedLabel="Confirm closing text recognition"
        className="h-6 w-6"
        idleIcon={<X size={15} />}
        idleLabel="Close text recognition"
        onConfirm={onClose}
      />
    </>
  );
}
