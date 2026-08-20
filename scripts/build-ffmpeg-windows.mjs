// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import {
  copyFile,
  mkdir,
  mkdtemp,
  readdir,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { arch, argv, exit, platform } from "node:process";

const builds = {
  arm64: {
    archiveUrl:
      "https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-08-20-13-45/ffmpeg-n8.1.2-44-g7c533d0f86-winarm64-gpl-8.1.zip",
    archiveSha256:
      "00152b582ea09b8a3a383b3020bc53b54113f8114a0da31f55d443500553ed3a",
    executableSha256:
      "364858dcabd2f0f5861275f1449c8da2d454f4d06034c75c41e57d6c42366a09",
  },
  x64: {
    archiveUrl:
      "https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-08-20-13-45/ffmpeg-n8.1.2-44-g7c533d0f86-win64-gpl-8.1.zip",
    archiveSha256:
      "410c82fc0a7d713fd83412138271b8559faa8cf8a74a75eaf541dfca75ea4590",
    executableSha256:
      "1698c91517032ceeed6df5427e1fc012adecb07e7c4a6b031efb8c0219d4891d",
  },
};

const sha256 = (path) =>
  new Promise((resolveDigest, reject) => {
    const hash = createHash("sha256");
    createReadStream(path)
      .on("error", reject)
      .on("data", (chunk) => hash.update(chunk))
      .on("end", () => resolveDigest(hash.digest("hex")));
  });

const findFfmpeg = async (directory) => {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      const nested = await findFfmpeg(path);
      if (nested) return nested;
    } else if (entry.name === "ffmpeg.exe") {
      return path;
    }
  }
};

if (platform !== "win32") {
  throw new Error("The Screenwide Windows FFmpeg builder must run on Windows");
}
if (argv.length !== 3) {
  throw new Error("usage: build-ffmpeg-windows.mjs OUTPUT");
}

const build = builds[arch];
if (!build) {
  throw new Error(`Screenwide does not package FFmpeg for Windows ${arch}`);
}
const { archiveSha256, archiveUrl, executableSha256 } = build;
const output = resolve(argv[2]);
if (
  await sha256(output)
    .then((digest) => digest === executableSha256)
    .catch(() => false)
) {
  exit(0);
}

const workDirectory = await mkdtemp(join(tmpdir(), "screenwide-ffmpeg-"));
try {
  const archive = join(workDirectory, "ffmpeg.zip");
  const response = await fetch(archiveUrl);
  if (!response.ok) {
    throw new Error(`FFmpeg download failed with HTTP ${response.status}`);
  }
  await writeFile(archive, new Uint8Array(await response.arrayBuffer()));
  if ((await sha256(archive)) !== archiveSha256) {
    throw new Error(
      "The downloaded FFmpeg archive failed SHA-256 verification",
    );
  }

  const extracted = join(workDirectory, "extracted");
  await mkdir(extracted);
  execFileSync("tar", ["-xf", archive, "-C", extracted], { stdio: "inherit" });
  const source = await findFfmpeg(extracted);
  if (!source || (await sha256(source)) !== executableSha256) {
    throw new Error(
      "The extracted FFmpeg executable failed SHA-256 verification",
    );
  }
  await mkdir(dirname(output), { recursive: true });
  await copyFile(source, output);
} finally {
  await rm(workDirectory, { force: true, recursive: true });
}
