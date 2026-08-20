// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { chmod, copyFile, mkdir } from "node:fs/promises";
import { arch as hostArch, env, platform as hostPlatform } from "node:process";
import { dirname, resolve } from "node:path";

const sha256 = (path) =>
  new Promise((resolveDigest, reject) => {
    const hash = createHash("sha256");
    createReadStream(path)
      .on("error", reject)
      .on("data", (chunk) => hash.update(chunk))
      .on("end", () => resolveDigest(hash.digest("hex")));
  });

const copyIfChanged = async (sourcePath, destinationPath, expectedHash) => {
  const unchanged = await sha256(destinationPath)
    .then((value) => value === expectedHash)
    .catch(() => false);
  if (unchanged) return;
  await mkdir(dirname(destinationPath), { recursive: true });
  await copyFile(sourcePath, destinationPath);
  if (hostPlatform !== "win32") await chmod(destinationPath, 0o755);
};

const triples = {
  "darwin-arm64": "aarch64-apple-darwin",
  "darwin-x64": "x86_64-apple-darwin",
  "win32-x64": "x86_64-pc-windows-msvc",
};
const nodeArchitectures = {
  aarch64: "arm64",
  arm64: "arm64",
  x86_64: "x64",
  x64: "x64",
};
const nodePlatforms = {
  darwin: "darwin",
  win32: "win32",
  windows: "win32",
};
const requestedPlatform = nodePlatforms[env.TAURI_ENV_PLATFORM ?? hostPlatform];
const requestedArch = nodeArchitectures[env.TAURI_ENV_ARCH ?? hostArch];
const requestedHost = `${requestedPlatform}-${requestedArch}`;
const actualHost = `${hostPlatform}-${hostArch}`;
const target = triples[requestedHost];

if (!target) {
  throw new Error(`Screenwide does not package FFmpeg for ${requestedHost}`);
}
if (requestedHost !== actualHost) {
  throw new Error(
    `FFmpeg preparation cannot use the ${actualHost} dependency for the requested ${requestedHost} target`,
  );
}
const extension = requestedPlatform === "win32" ? ".exe" : "";
const destination = resolve(
  "src-tauri",
  "binaries",
  `ffmpeg-${target}${extension}`,
);
if (requestedPlatform === "darwin") {
  execFileSync("sh", ["scripts/build-ffmpeg-macos.sh", destination], {
    stdio: "inherit",
  });
} else {
  execFileSync("node", ["scripts/build-ffmpeg-windows.mjs", destination], {
    stdio: "inherit",
  });
}
const digest = await sha256(destination);

// Tauri's externalBin copy is what release bundles use. Keeping the current
// development target beside the app as well makes a hot-restarted `tauri dev`
// process exercise the same runtime resolver without relying on PATH.
const debugDestination = resolve(
  "src-tauri",
  "target",
  "debug",
  `ffmpeg${extension}`,
);
await copyIfChanged(destination, debugDestination, digest);

console.log(`Prepared bundled FFmpeg for ${target}`);
