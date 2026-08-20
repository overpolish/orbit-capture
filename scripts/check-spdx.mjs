// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { extname } from "node:path";
import process from "node:process";

const COPYRIGHT = "SPDX-FileCopyrightText: 2026 overpolish";
const LICENSE = "SPDX-License-Identifier: GPL-3.0-or-later";
const CHECKED_EXTENSIONS = new Set([
  ".css",
  ".html",
  ".js",
  ".md",
  ".mjs",
  ".plist",
  ".rs",
  ".svg",
  ".toml",
  ".ts",
  ".tsx",
  ".yaml",
  ".yml",
]);
const EXCLUDED_FILES = new Set(["pnpm-lock.yaml"]);
const EXCLUDED_DIRECTORIES = ["src-tauri/vendor/"];

const trackedFiles = execFileSync(
  "git",
  ["ls-files", "--cached", "--others", "--exclude-standard", "-z"],
  { encoding: "utf8" },
)
  .split("\0")
  .filter(Boolean);

const missingHeaders = trackedFiles.filter((file) => {
  if (
    !existsSync(file) ||
    EXCLUDED_FILES.has(file) ||
    EXCLUDED_DIRECTORIES.some((directory) => file.startsWith(directory)) ||
    !CHECKED_EXTENSIONS.has(extname(file))
  ) {
    return false;
  }

  const header = readFileSync(file, "utf8").split(/\r?\n/, 12).join("\n");
  return !header.includes(COPYRIGHT) || !header.includes(LICENSE);
});

if (missingHeaders.length > 0) {
  console.error("These tracked files are missing their SPDX header:");
  for (const file of missingHeaders) console.error(`- ${file}`);
  process.exitCode = 1;
}
