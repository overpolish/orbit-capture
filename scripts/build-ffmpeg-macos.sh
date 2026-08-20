#!/bin/sh
# SPDX-FileCopyrightText: 2026 overpolish
# SPDX-License-Identifier: GPL-3.0-or-later

set -eu

if [ "$(uname -s)" != "Darwin" ]; then
  echo "The Screenwide macOS FFmpeg builder must run on macOS" >&2
  exit 1
fi
if [ "$(uname -m)" != "arm64" ]; then
  echo "Screenwide packages FFmpeg only for Apple Silicon macOS" >&2
  exit 1
fi
if [ "$#" -ne 1 ]; then
  echo "usage: build-ffmpeg-macos.sh OUTPUT" >&2
  exit 1
fi

output=$1
build_id=ffmpeg-9.0.1-x264-b35605ace3ddf7c1a5d67a2eb553f034aef41d55-macos-v5
stamp="$output.build-id"
if [ -x "$output" ] && [ -f "$stamp" ] && [ "$(cat "$stamp")" = "$build_id" ] && \
  "$output" -hide_banner -version 2>/dev/null | grep -q "ffmpeg version 9.0.1" && \
  "$output" -hide_banner -version 2>/dev/null | grep -q -- "--disable-autodetect" && \
  ! "$output" -hide_banner -L 2>&1 | grep -q "not legally redistributable" && \
  "$output" -hide_banner -encoders 2>/dev/null | grep -q "libx264"; then
  exit 0
fi

ffmpeg_version=9.0.1
ffmpeg_sha256=cf38e0e28c7e5605942c4a77755349b0145804a397af37eb1fb4c77cb237f635
x264_revision=b35605ace3ddf7c1a5d67a2eb553f034aef41d55
x264_sha256=cd71a7515b0e9a012e1ac9b1f8415bebcaf6fc97d4db32286642ac4c0fbe24f9
deployment_target=14.2

work_dir=$(mktemp -d "${TMPDIR:-/tmp}/screenwide-ffmpeg.XXXXXX")
trap 'rm -rf "$work_dir"' EXIT HUP INT TERM
prefix="$work_dir/prefix"
ffmpeg_archive="$work_dir/ffmpeg.tar.xz"
x264_archive="$work_dir/x264.tar.gz"

curl --fail --location --silent --show-error \
  "https://ffmpeg.org/releases/ffmpeg-$ffmpeg_version.tar.xz" \
  --output "$ffmpeg_archive"
curl --fail --location --silent --show-error \
  "https://code.videolan.org/videolan/x264/-/archive/$x264_revision/x264-$x264_revision.tar.gz" \
  --output "$x264_archive"

printf '%s  %s\n' "$ffmpeg_sha256" "$ffmpeg_archive" | shasum -a 256 -c -
printf '%s  %s\n' "$x264_sha256" "$x264_archive" | shasum -a 256 -c -

tar -xf "$x264_archive" -C "$work_dir"
x264_source="$work_dir/x264-$x264_revision"
(
  cd "$x264_source"
  MACOSX_DEPLOYMENT_TARGET=$deployment_target ./configure \
    "--extra-asflags=-mmacosx-version-min=$deployment_target" \
    --prefix="$prefix" \
    --enable-static \
    --enable-pic \
    --extra-cflags="-mmacosx-version-min=$deployment_target" \
    --extra-ldflags="-mmacosx-version-min=$deployment_target" \
    --disable-opencl \
    --disable-cli
  make -s -j"$(sysctl -n hw.logicalcpu)"
  make -s install
)

tar -xf "$ffmpeg_archive" -C "$work_dir"
ffmpeg_source="$work_dir/ffmpeg-$ffmpeg_version"
(
  cd "$ffmpeg_source"
  MACOSX_DEPLOYMENT_TARGET=$deployment_target \
  PKG_CONFIG_PATH="$prefix/lib/pkgconfig" \
  ./configure \
    --prefix=/screenwide-ffmpeg \
    --pkg-config-flags=--static \
    --extra-cflags="-mmacosx-version-min=$deployment_target" \
    --extra-ldflags="-mmacosx-version-min=$deployment_target" \
    --enable-static \
    --disable-shared \
    --disable-autodetect \
    --enable-gpl \
    --enable-version3 \
    --enable-libx264 \
    --enable-audiotoolbox \
    --enable-videotoolbox \
    --disable-doc \
    --disable-debug \
    --disable-ffplay \
    --disable-ffprobe \
    --disable-network
  make -s -j"$(sysctl -n hw.logicalcpu)" ffmpeg
)

mkdir -p "$(dirname "$output")"
cp "$ffmpeg_source/ffmpeg" "$output"
chmod 755 "$output"
printf '%s\n' "$build_id" > "$stamp"
