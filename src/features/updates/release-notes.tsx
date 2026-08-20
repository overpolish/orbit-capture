// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later
/* eslint-disable @eslint-react/dom-no-dangerously-set-innerhtml -- GitHub's body_html is sanitized by GitHub before it reaches the app. */

import { openUrl } from "@tauri-apps/plugin-opener";

type ReleaseNotesProps = {
  html: string;
};

const externalUrl = (rawUrl: string) => {
  try {
    const url = new URL(rawUrl);
    return url.protocol === "https:" || url.protocol === "http:" ? url : null;
  } catch {
    return null;
  }
};

/** Render HTML sanitized by GitHub's release API and open links externally. */
export function ReleaseNotes({ html }: ReleaseNotesProps) {
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
    />
  );
}
