// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";

const DEFAULT_LIMIT = 400;
const TEST_LIMIT = 650;
const roots = ["src", "src-tauri/src", "scripts"];
const extensions = new Set([".mjs", ".rs", ".ts", ".tsx"]);

// These are cohesive state-machine/controller seams that were deliberately
// kept together during the architecture pass. Their caps match the current
// shape closely, so they cannot become a place to quietly add more concerns.
const exceptions = new Map([
  ["src-tauri/src/windows.rs", 600],
  ["src/features/exports/components/preview-viewport.tsx", 625],
  ["src/features/exports/components/scrub-preview.tsx", 700],
  ["src/features/exports/export-window.tsx", 555],
  ["src-tauri/src/screenshots/output.rs", 435],
  ["src/features/exports/components/export-inspector.tsx", 405],
  ["src/features/exports/components/native-recording-preview.tsx", 410],
]);

const files = [];

async function visit(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  for (const entry of entries) {
    const file = path.join(directory, entry.name);
    if (entry.isDirectory()) await visit(file);
    else if (extensions.has(path.extname(entry.name))) files.push(file);
  }
}

await Promise.all(roots.map(visit));

const failures = [];
for (const file of files.sort()) {
  const source = await readFile(file, "utf8");
  const lines = source.length === 0 ? 0 : source.split(/\r?\n/u).length;
  const normalized = file.split(path.sep).join("/");
  const isTest = /(?:^|\/)(?:tests?\.rs|[^/]+_tests\.rs|tests\/)/u.test(
    normalized,
  );
  const limit =
    exceptions.get(normalized) ?? (isTest ? TEST_LIMIT : DEFAULT_LIMIT);
  if (lines > limit)
    failures.push(`${normalized}: ${lines.toString()} > ${limit.toString()}`);
}

if (failures.length > 0) {
  console.error("Source files exceed their maintainability limit:\n");
  console.error(failures.map((failure) => `- ${failure}`).join("\n"));
  console.error(
    "\nSplit the responsibility or deliberately tighten/update the exception.",
  );
  process.exitCode = 1;
} else {
  console.log(`Source-size check passed (${files.length.toString()} files).`);
}
