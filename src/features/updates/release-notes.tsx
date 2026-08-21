// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later
/* eslint-disable @eslint-react/dom-no-dangerously-set-innerhtml -- GitHub's body_html is sanitized by GitHub before it reaches the app. */

import { openUrl } from "@tauri-apps/plugin-opener";
import { memo, type RefObject, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { Checkbox } from "../../components/base/checkbox/checkbox";

type ReleaseNotesProps = {
  html: string;
};

type TaskMarker = {
  input: HTMLInputElement;
  label: string;
  selected: boolean;
  target: HTMLSpanElement;
};

type SanitizedReleaseNotesProps = ReleaseNotesProps & {
  notesRef: RefObject<HTMLDivElement | null>;
};

const externalUrl = (rawUrl: string) => {
  try {
    const url = new URL(rawUrl);
    return url.protocol === "https:" || url.protocol === "http:" ? url : null;
  } catch {
    return null;
  }
};

const SanitizedReleaseNotes = memo(function SanitizedReleaseNotes({
  html,
  notesRef,
}: SanitizedReleaseNotesProps) {
  return (
    <div
      className="release-notes"
      // GitHub returns `body_html` only after sanitizing the release Markdown.
      dangerouslySetInnerHTML={{ __html: html }}
      onClick={(event) => {
        const anchor = (event.target as HTMLElement).closest<HTMLAnchorElement>(
          "a[href]",
        );
        if (!anchor) return;
        const url = externalUrl(anchor.href);
        if (!url) return;
        event.preventDefault();
        void openUrl(url.toString());
      }}
      ref={notesRef}
    />
  );
});

/** Render HTML sanitized by GitHub's release API and open links externally. */
export function ReleaseNotes({ html }: ReleaseNotesProps) {
  const notesRef = useRef<HTMLDivElement>(null);
  const [taskMarkers, setTaskMarkers] = useState<TaskMarker[]>([]);

  useLayoutEffect(() => {
    const notes = notesRef.current;
    if (!notes) return;

    const markers = Array.from(
      notes.querySelectorAll<HTMLInputElement>("input.task-list-item-checkbox"),
    ).map((input) => {
      const target = document.createElement("span");
      target.className = "release-notes-task-marker";
      input.before(target);
      input.hidden = true;

      return {
        input,
        label:
          input.getAttribute("aria-label") ??
          input.parentElement?.textContent.trim() ??
          "Task",
        selected: input.checked,
        target,
      };
    });

    setTaskMarkers(markers);

    return () => {
      for (const marker of markers) {
        marker.input.hidden = false;
        marker.target.remove();
      }
    };
  }, [html]);

  return (
    <>
      <SanitizedReleaseNotes html={html} notesRef={notesRef} />
      {taskMarkers.map(({ label, selected, target }, index) =>
        createPortal(
          <Checkbox
            aria-label={label}
            isReadOnly
            isSelected={selected}
            size="xs"
          />,
          target,
          `${label}-${String(index)}`,
        ),
      )}
    </>
  );
}
