<!-- SPDX-FileCopyrightText: 2026 overpolish -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# FFmpeg distribution

Screenwide distributes FFmpeg as a separate executable under the GNU General Public License. Screenwide verifies that every packaged executable includes the `libx264` encoder and was not configured with `--enable-nonfree`.

## macOS

The release build is compiled locally by `scripts/build-ffmpeg-macos.sh` from:

- FFmpeg 9.0.1: <https://ffmpeg.org/releases/ffmpeg-9.0.1.tar.xz>
- x264 revision `b35605ace3ddf7c1a5d67a2eb553f034aef41d55`: <https://code.videolan.org/videolan/x264>

The script records and verifies the source archive hashes and contains the full configuration used to produce the executable.

Source downloads are retried only when their pinned SHA-256 does not match. If VideoLAN's x264 archive endpoint remains unavailable or inconsistent, the script downloads the identical commit archive from the GitHub x264 mirror and applies the same checksum verification.

## Windows

The x86-64 and ARM64 releases use BtbN's pinned static GPL builds `n8.1.2-44-g7c533d0f86` from release `autobuild-2026-08-20-13-45`:

<https://github.com/BtbN/FFmpeg-Builds/releases/tag/autobuild-2026-08-20-13-45>

`scripts/build-ffmpeg-windows.mjs` records and verifies both the archive and extracted executable hashes. The corresponding build scripts and FFmpeg source revision are available from <https://github.com/BtbN/FFmpeg-Builds>.

The FFmpeg notices and GPL text are included in Screenwide's application resources. The complete corresponding FFmpeg and x264 sources must also remain available with every public Screenwide binary distribution.
