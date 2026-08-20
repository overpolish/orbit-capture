// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { readFile } from "node:fs/promises";
import { argv } from "node:process";

const readJson = async (path) => JSON.parse(await readFile(path, "utf8"));
const packageJson = await readJson("package.json");
const tauriConfig = await readJson("src-tauri/tauri.conf.json");
const cargoToml = await readFile("src-tauri/Cargo.toml", "utf8");
const cargoVersion = cargoToml.match(/^version\s*=\s*"([^"]+)"/m)?.[1];

const versions = new Map([
  ["package.json", packageJson.version],
  ["src-tauri/Cargo.toml", cargoVersion],
  ["src-tauri/tauri.conf.json", tauriConfig.version],
]);
const expected = packageJson.version;

for (const [path, version] of versions) {
  if (version !== expected) {
    throw new Error(`${path} is ${String(version)}; expected ${expected}`);
  }
}

const tag = argv[2];
if (tag && tag !== `v${expected}`) {
  throw new Error(`release tag ${tag} does not match app version v${expected}`);
}

console.log(`Release version ${expected} is consistent`);
