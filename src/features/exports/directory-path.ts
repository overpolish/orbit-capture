// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

const ELLIPSIS = "...";

/**
 * Middle-truncate a raw string, keeping its head and tail around an ellipsis.
 * Used as the graceful fallback for paths that are really one long segment (or
 * whose final segment alone already overflows the cap).
 */
const middleTruncate = (value: string, maxLength: number): string => {
  if (value.length <= maxLength) return value;
  if (maxLength <= ELLIPSIS.length) return value.slice(0, maxLength);
  const keep = maxLength - ELLIPSIS.length;
  const front = Math.ceil(keep / 2);
  const back = Math.floor(keep / 2);
  return value.slice(0, front) + ELLIPSIS + value.slice(value.length - back);
};

/**
 * A destination path shortened for the titlebar's folder button.
 *
 * Paths within `maxLength` are returned unchanged. Longer ones keep the root
 * segment and as many trailing segments as fit around an ellipsis, e.g.
 * `/Users/.../2026/August` or `C:\...\Some Folder\Final Folder`, so the user
 * still recognizes both the volume/root and the leaf they are exporting into.
 *
 * The path's own separator is preserved so it reads natively on each platform:
 * a Windows path (any `\` present) is rejoined with `\`, otherwise `/`. A
 * leading root separator is kept too, so an absolute POSIX path shows
 * `/Users/...` rather than losing its leading slash; Windows paths (`C:\...`)
 * have no leading separator and keep the volume (`C:`) as their root.
 *
 * The ellipsis lands only at separator boundaries, never inside a segment, so
 * the result is stable regardless of where a name happens to contain a space —
 * unlike CSS `text-overflow: ellipsis`, which WebKit renders with a stray gap
 * when the clip point falls on whitespace.
 */
export const truncateDirectoryPath = (
  directory: string,
  maxLength = 30,
): string => {
  if (directory.length <= maxLength) return directory;

  // Present the path with its own separator (Windows `\`, otherwise `/`) so it
  // matches how the OS shows it; a leading separator is kept for POSIX roots.
  const separator = directory.includes("\\") ? "\\" : "/";
  const rootSeparator = /^[\\/]/.test(directory) ? separator : "";
  const segments = directory.split(/[\\/]/).filter(Boolean);
  if (segments.length <= 1) return middleTruncate(directory, maxLength);

  const root = rootSeparator + segments[0];
  const last = segments[segments.length - 1];

  // Floor: root + ellipsis + leaf. Grow inward from the leaf while it fits.
  const tail = [last];
  for (let index = segments.length - 2; index >= 1; index--) {
    const candidate = [root, ELLIPSIS, segments[index], ...tail].join(
      separator,
    );
    if (candidate.length > maxLength) break;
    tail.unshift(segments[index]);
  }

  const result = [root, ELLIPSIS, ...tail].join(separator);
  // Even root + ellipsis + leaf can overflow when the leaf is huge; degrade to
  // a plain middle-truncation of the whole path so something legible remains.
  return result.length > maxLength
    ? middleTruncate(directory, maxLength)
    : result;
};
