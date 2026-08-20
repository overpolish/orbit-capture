// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { readFile } from "node:fs/promises";
import { argv, env, exit, platform } from "node:process";
import { spawnSync } from "node:child_process";

const arguments_ = argv.slice(2);
if (arguments_[0] === "--") arguments_.shift();
const tag = arguments_[0];
if (!tag || !/^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(tag)) {
  console.error(
    "Usage: pnpm update:staging -- v0.2.0-rc.1 [tauri build options]",
  );
  exit(1);
}
if (!["darwin", "win32"].includes(platform)) {
  console.error(
    "Screenwide staging update clients are supported on macOS and Windows.",
  );
  exit(1);
}

const packageJson = JSON.parse(await readFile("package.json", "utf8"));
const endpoint = `https://github.com/overpolish/screenwide/releases/download/${encodeURIComponent(tag)}/latest.json`;
const override = JSON.stringify({
  bundle: { createUpdaterArtifacts: false },
  plugins: { updater: { endpoints: [endpoint] } },
});
const extraOptions = arguments_.slice(1);
const hasBundleOption = extraOptions.some(
  (option) =>
    option === "--bundles" ||
    option === "-b" ||
    option.startsWith("--bundles="),
);
const defaultBundles = hasBundleOption
  ? []
  : ["--bundles", platform === "win32" ? "nsis" : "app"];
const command = platform === "win32" ? "pnpm.cmd" : "pnpm";

console.log(
  `Building Screenwide ${String(packageJson.version)} as an updater test client`,
);
console.log(`Staging update endpoint: ${endpoint}`);
console.log(
  "The release must be published (a GitHub draft is not downloadable by the app).\n",
);

const result = spawnSync(
  command,
  [
    "tauri",
    "build",
    "--features",
    "tauri/devtools",
    "--config",
    override,
    ...defaultBundles,
    ...extraOptions,
  ],
  {
    env: { ...env, VITE_SCREENWIDE_UPDATE_DEBUG: "1" },
    stdio: "inherit",
  },
);

if (result.error) throw result.error;
exit(result.status ?? 1);
