#!/bin/sh
# SPDX-FileCopyrightText: 2026 overpolish
# SPDX-License-Identifier: GPL-3.0-or-later

set -eu

binary=$1
shift

case "$binary" in
  /*) ;;
  *) binary="$(pwd)/$binary" ;;
esac

script_directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
workspace_directory=$(dirname -- "$script_directory")
app_directory="$(dirname -- "$binary")/Orbit Capture.app"
app_executable="$app_directory/Contents/MacOS/orbit-capture"

mkdir -p "$app_directory/Contents/MacOS" "$app_directory/Contents/Resources"
cp "$script_directory/macos-dev-info.plist" "$app_directory/Contents/Info.plist"
cp "$workspace_directory/src-tauri/icons/icon.icns" "$app_directory/Contents/Resources/icon.icns"
ln -sfn "$binary" "$app_executable"

exec "$app_executable" "$@"
