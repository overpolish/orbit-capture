// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

const releasesApi =
  "https://api.github.com/repos/overpolish/screenwide/releases/tags";

type GitHubRelease = {
  body_html?: unknown;
};

/** Fetch GitHub's sanitized HTML for the exact release Tauri found. */
export async function fetchReleaseNotesHtml(
  version: string,
): Promise<string | null> {
  const tag = version.startsWith("v") ? version : `v${version}`;
  try {
    const response = await fetch(`${releasesApi}/${encodeURIComponent(tag)}`, {
      headers: { Accept: "application/vnd.github.html+json" },
    });
    if (!response.ok) return null;
    const release = (await response.json()) as GitHubRelease;
    return typeof release.body_html === "string" && release.body_html.trim()
      ? release.body_html
      : null;
  } catch {
    return null;
  }
}
