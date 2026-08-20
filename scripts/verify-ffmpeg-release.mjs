// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { execFileSync } from "node:child_process";
import { arch, env, platform } from "node:process";
import { resolve } from "node:path";

const targets = {
  "darwin-arm64": "aarch64-apple-darwin",
  "darwin-x64": "x86_64-apple-darwin",
  "win32-x64": "x86_64-pc-windows-msvc",
};
const architectures = {
  aarch64: "arm64",
  arm64: "arm64",
  x86_64: "x64",
  x64: "x64",
};
const platforms = { darwin: "darwin", win32: "win32", windows: "win32" };
const requestedPlatform = platforms[env.TAURI_ENV_PLATFORM ?? platform];
const requestedArch = architectures[env.TAURI_ENV_ARCH ?? arch];
const target = targets[`${requestedPlatform}-${requestedArch}`];
const extension = requestedPlatform === "win32" ? ".exe" : "";
const ffmpeg = target
  ? resolve("src-tauri", "binaries", `ffmpeg-${target}${extension}`)
  : undefined;

if (typeof ffmpeg !== "string" || ffmpeg.length === 0) {
  throw new Error("The release FFmpeg binary is unavailable");
}

const output = (args) =>
  execFileSync(ffmpeg, args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
const version = output(["-hide_banner", "-version"]);
const license = output(["-hide_banner", "-L"]);
const encoders = output(["-hide_banner", "-encoders"]);
const filters = output(["-hide_banner", "-filters"]);
const formats = output(["-hide_banner", "-formats"]);

if (
  version.includes("--enable-nonfree") ||
  license.includes("not legally redistributable")
) {
  throw new Error(
    "The selected FFmpeg build contains nonfree parts and cannot be included in a Screenwide release",
  );
}
const hasCapability = (listing, name) =>
  listing.split("\n").some((line) => line.split(/[\s,]+/).includes(name));
for (const encoder of ["aac", "libx264"]) {
  if (hasCapability(encoders, encoder)) continue;
  throw new Error(
    `The selected FFmpeg build lacks the ${encoder} encoder required by Screenwide`,
  );
}
for (const filter of [
  "adelay",
  "aformat",
  "alimiter",
  "amix",
  "anull",
  "apad",
  "aresample",
  "atrim",
  "concat",
  "scale",
  "setpts",
]) {
  if (hasCapability(filters, filter)) continue;
  throw new Error(
    `The selected FFmpeg build lacks the ${filter} filter required by Screenwide`,
  );
}
for (const format of ["f32le", "h264", "mov", "mp4"]) {
  if (hasCapability(formats, format)) continue;
  throw new Error(
    `The selected FFmpeg build lacks the ${format} format required by Screenwide`,
  );
}
if (requestedPlatform === "darwin") {
  const linkedLibraries = execFileSync("otool", ["-L", ffmpeg], {
    encoding: "utf8",
  });
  if (
    version.includes("--enable-nonfree") ||
    /\s(?:\/opt\/|\/usr\/local\/)/.test(linkedLibraries)
  ) {
    throw new Error(
      "The selected macOS FFmpeg build links to a package-manager library",
    );
  }
}

console.log("Verified release-safe FFmpeg configuration");
