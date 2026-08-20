// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";

const DEFAULT_LIMIT = 300;
const TEST_LIMIT = 650;
const roots = ["src", "src-tauri/src", "scripts"];
const extensions = new Set([".h", ".m", ".mjs", ".rs", ".ts", ".tsx"]);

// Oversized legacy files are frozen at their surveyed size. Refactors remove
// entries rather than raising ceilings, so new code and completed splits use
// the normal limit while the remaining debt cannot quietly grow.
const debtCeilings = new Map(
  Object.entries(
    JSON.parse(await readFile("scripts/source-size-debt.json", "utf8")),
  ),
);

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
const visited = new Set();
for (const file of files.sort()) {
  const source = await readFile(file, "utf8");
  const lines = source.length === 0 ? 0 : source.split(/\r?\n/u).length;
  const normalized = file.split(path.sep).join("/");
  visited.add(normalized);
  const isTest = /(?:^|\/)(?:tests?\.rs|[^/]+_tests\.rs|tests\/)/u.test(
    normalized,
  );
  const standardLimit = isTest ? TEST_LIMIT : DEFAULT_LIMIT;
  const debtCeiling = debtCeilings.get(normalized);
  const limit = debtCeiling ?? standardLimit;
  if (lines > limit)
    failures.push(`${normalized}: ${lines.toString()} > ${limit.toString()}`);
  else if (debtCeiling !== undefined && lines <= standardLimit)
    failures.push(`${normalized}: remove its cleared source-size debt entry`);
}

for (const file of debtCeilings.keys()) {
  if (!visited.has(file))
    failures.push(`${file}: remove its stale source-size debt entry`);
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
