// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn video_encoding_args(video: VideoExportOptions) -> Vec<OsString> {
  let VideoExportOptions {
    compression,
    resolution_scale_percent,
    source_scale_percent,
  } = video;
  let scale_filter = resolution_filter(source_scale_percent, resolution_scale_percent);
  if let Some(crf) = export_crf(compression, scale_filter.is_some()) {
    let mut args = [
      "-c:v",
      "libx264",
      "-preset",
      "medium",
      "-crf",
      &crf.to_string(),
      "-pix_fmt",
      "yuv420p",
      "-profile:v",
      "high",
    ]
    .map(OsString::from)
    .to_vec();
    if let Some(filter) = scale_filter {
      args.extend([OsString::from("-vf"), OsString::from(filter)]);
    }
    args
  } else {
    ["-c:v", "copy"].map(OsString::from).to_vec()
  }
}

pub(in crate::exports::media_preview) fn remux_args(
  source: &Path,
  destination: &Path,
) -> Vec<OsString> {
  let mut args: Vec<OsString> = ["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"]
    .map(OsString::from)
    .into();
  args.push(source.into());
  args.extend(["-map", "0", "-c", "copy"].map(OsString::from));
  args.extend(EXPORT_MP4_OUTPUT.map(OsString::from));
  args.push(destination.into());
  args
}

/// FFmpeg arguments for an export that differs from the source. Video is
/// stream-copied at Original and quality-encoded at every compression level;
/// selected audio is decoded only when several tracks must become one.
pub(in crate::exports::media_preview) fn selected_export_args(
  source: &Path,
  destination: &Path,
  selection: &TrackSelection,
  layout: AudioLayout,
  video: VideoExportOptions,
) -> Vec<OsString> {
  let mut args: Vec<OsString> = ["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"]
    .map(OsString::from)
    .into();
  args.push(source.into());
  // Machine-readable progress belongs on the save encode, not on preview
  // mixes. It is written to stdout while diagnostics continue to use stderr.
  args.extend(["-progress", "pipe:1", "-nostats"].map(OsString::from));
  args.extend(["-map", "0:v:0?"].map(OsString::from));
  args.extend(video_encoding_args(video));
  args.extend(selection.audio_args(layout).into_iter().map(OsString::from));
  args.extend(EXPORT_MP4_OUTPUT.map(OsString::from));
  args.push(destination.into());
  args
}

pub(in crate::exports::media_preview) fn camera_export_args(
  audio_source: &Path,
  camera_source: &Path,
  destination: &Path,
  selection: &TrackSelection,
  layout: AudioLayout,
  video: VideoExportOptions,
) -> Vec<OsString> {
  let mut args: Vec<OsString> = ["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"]
    .map(OsString::from)
    .into();
  args.push(audio_source.into());
  args.push(OsString::from("-i"));
  args.push(camera_source.into());
  args.extend(["-progress", "pipe:1", "-nostats"].map(OsString::from));
  args.extend(["-map", "1:v:0"].map(OsString::from));
  args.extend(video_encoding_args(video));
  args.extend(
    selection
      .audio_args_from(layout, 0)
      .into_iter()
      .map(OsString::from),
  );
  args.extend(EXPORT_MP4_OUTPUT.map(OsString::from));
  args.push(destination.into());
  args
}

pub(in crate::exports::media_preview) fn audio_export_args(
  source: &Path,
  destination: &Path,
  selection: &TrackSelection,
  layout: AudioLayout,
) -> Vec<OsString> {
  let mut args: Vec<OsString> = ["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"]
    .map(OsString::from)
    .into();
  args.push(source.into());
  args.extend(["-progress", "pipe:1", "-nostats"].map(OsString::from));
  args.extend(selection.audio_args(layout).into_iter().map(OsString::from));
  args.extend(EXPORT_MP4_OUTPUT.map(OsString::from));
  args.push(destination.into());
  args
}
