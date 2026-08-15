// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { chmod, copyFile, mkdir } from "node:fs/promises";
import { createRequire } from "node:module";
import { arch, platform } from "node:process";
import { dirname, resolve } from "node:path";

const require = createRequire(import.meta.url);
const source = require("ffmpeg-static");

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
  if (platform !== "win32") await chmod(destinationPath, 0o755);
};

const triples = {
  "darwin-arm64": "aarch64-apple-darwin",
  "darwin-x64": "x86_64-apple-darwin",
  "win32-x64": "x86_64-pc-windows-msvc",
};
const hashes = {
  "darwin-arm64":
    "a90e3db6a3fd35f6074b013f948b1aa45b31c6375489d39e572bea3f18336584",
  "darwin-x64":
    "ebdddc936f61e14049a2d4b549a412b8a40deeff6540e58a9f2a2da9e6b18894",
  "win32-x64":
    "04e1307997530f9cf2fe35cba2ca7e8875ca91da02f89d6c7243df819c94ad00",
};
const target = triples[`${platform}-${arch}`];

if (!target) {
  throw new Error(`Screenwide does not package FFmpeg for ${platform}-${arch}`);
}
if (typeof source !== "string" || source.length === 0) {
  throw new Error("ffmpeg-static did not provide a binary for this platform");
}

const digest = await sha256(source);
if (digest !== hashes[`${platform}-${arch}`]) {
  throw new Error(`The downloaded FFmpeg binary failed SHA-256 verification`);
}

const extension = platform === "win32" ? ".exe" : "";
const destination = resolve(
  "src-tauri",
  "binaries",
  `ffmpeg-${target}${extension}`,
);
await copyIfChanged(source, destination, digest);

// Tauri's externalBin copy is what release bundles use. Keeping the current
// development target beside the app as well makes a hot-restarted `tauri dev`
// process exercise the same runtime resolver without relying on PATH.
const debugDestination = resolve(
  "src-tauri",
  "target",
  "debug",
  `ffmpeg${extension}`,
);
await copyIfChanged(source, debugDestination, digest);

console.log(`Prepared bundled FFmpeg for ${target}`);
