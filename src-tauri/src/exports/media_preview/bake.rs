// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn even_scaled(value: u32, numerator: u16, denominator: u16) -> u32 {
  let scaled = u64::from(value)
    .saturating_mul(u64::from(numerator))
    .checked_div(u64::from(denominator.max(1)))
    .unwrap_or(0)
    .max(2);
  (scaled.min(u64::from(u32::MAX)) as u32) & !1
}

#[derive(Clone, Copy, Debug)]
struct BakeGeometry {
  crop_height: u32,
  crop_width: u32,
  crop_x: u32,
  crop_y: u32,
  frame_height: u32,
  frame_width: u32,
  frame_x: u32,
  frame_y: u32,
  output_height: u32,
  output_width: u32,
  radius: u32,
}

fn even(value: f64) -> u32 {
  ((value.round().max(2.0) as u32) & !1).max(2)
}

fn bake_geometry(options: BakedVideoExportOptions) -> Result<BakeGeometry, String> {
  let output_width = even_scaled(
    options.screen_width,
    options.video.resolution_scale_percent,
    options.video.source_scale_percent,
  );
  let output_height = even_scaled(
    options.screen_height,
    options.video.resolution_scale_percent,
    options.video.source_scale_percent,
  );
  let frame_x = f64::from(output_width) * options.overlay.frame_x_percent / 100.0;
  let frame_y = f64::from(output_height) * options.overlay.frame_y_percent / 100.0;
  let frame_width = f64::from(output_width) * options.overlay.frame_width_percent / 100.0;
  let frame_height = f64::from(output_height) * options.overlay.frame_height_percent / 100.0;
  let camera_width = f64::from(output_width) * options.overlay.camera_width_percent / 100.0;
  let camera_height =
    camera_width * f64::from(options.camera_height) / f64::from(options.camera_width.max(1));
  let camera_x =
    f64::from(output_width) * options.overlay.camera_x_percent / 100.0 - camera_width / 2.0;
  let camera_y =
    f64::from(output_height) * options.overlay.camera_y_percent / 100.0 - camera_height / 2.0;
  let epsilon = 0.01;
  if frame_x + epsilon < camera_x
    || frame_y + epsilon < camera_y
    || frame_x + frame_width > camera_x + camera_width + epsilon
    || frame_y + frame_height > camera_y + camera_height + epsilon
  {
    return Err("The camera image no longer covers its crop window".to_owned());
  }

  let source_scale = f64::from(options.camera_width.max(1)) / camera_width.max(1.0);
  let source_x = (frame_x - camera_x) * source_scale;
  let source_y = (frame_y - camera_y) * source_scale;
  let source_width = frame_width * source_scale;
  let source_height = frame_height * source_scale;
  let crop_x = (source_x.round().max(0.0) as u32) & !1;
  let crop_y = (source_y.round().max(0.0) as u32) & !1;
  let crop_width = even(source_width)
    .min(options.camera_width.saturating_sub(crop_x) & !1)
    .max(2);
  let crop_height = even(source_height)
    .min(options.camera_height.saturating_sub(crop_y) & !1)
    .max(2);
  let frame_width = even(frame_width);
  let frame_height = even(frame_height);
  Ok(BakeGeometry {
    crop_height,
    crop_width,
    crop_x,
    crop_y,
    frame_height,
    frame_width,
    frame_x: frame_x.round().max(0.0) as u32,
    frame_y: frame_y.round().max(0.0) as u32,
    output_height,
    output_width,
    radius: (f64::from(frame_width.min(frame_height)) * options.overlay.radius_percent / 100.0)
      .round() as u32,
  })
}

fn rounded_alpha(radius: u32) -> String {
  if radius == 0 {
    return "255".to_owned();
  }
  format!(
    "if(lte(hypot(max(0\\,{radius}-min(X\\,W-1-X))\\,max(0\\,{radius}-min(Y\\,H-1-Y)))\\,{radius})\\,255\\,0)"
  )
}

pub(super) fn baked_export_args(
  screen: &Path,
  camera: &Path,
  destination: &Path,
  selection: &TrackSelection,
  layout: AudioLayout,
  options: BakedVideoExportOptions,
) -> Result<Vec<OsString>, String> {
  let geometry = bake_geometry(options)?;
  let alpha = rounded_alpha(geometry.radius);
  let BakeGeometry {
    crop_height,
    crop_width,
    crop_x,
    crop_y,
    frame_height,
    frame_width,
    frame_x,
    frame_y,
    output_height,
    output_width,
    ..
  } = geometry;
  let video_filter = format!(
    "[0:v:0]setpts=PTS-STARTPTS,scale={output_width}:{output_height}:flags=lanczos,setsar=1[screen];\
     [1:v:0]setpts=PTS-STARTPTS,crop={crop_width}:{crop_height}:{crop_x}:{crop_y},\
     scale={frame_width}:{frame_height}:flags=lanczos,format=rgba,\
     geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='{alpha}'[camera];\
     [screen][camera]overlay={frame_x}:{frame_y}:shortest=0:eof_action=repeat:format=auto[video]"
  );
  let mut audio_args = selection.audio_args(layout);
  let filter = if audio_args
    .first()
    .is_some_and(|argument| argument == "-filter_complex")
  {
    let audio_filter = audio_args.remove(1);
    audio_args.remove(0);
    format!("{video_filter};{audio_filter}")
  } else {
    video_filter
  };

  let mut args: Vec<OsString> = ["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"]
    .map(OsString::from)
    .into();
  args.push(screen.into());
  args.push(OsString::from("-i"));
  args.push(camera.into());
  args.extend(["-progress", "pipe:1", "-nostats", "-filter_complex"].map(OsString::from));
  args.push(filter.into());
  args.extend(
    [
      "-map", "[video]", "-c:v", "libx264", "-preset", "medium", "-crf",
    ]
    .map(OsString::from),
  );
  args.push(
    export_crf(options.video.compression, true)
      .unwrap_or(20)
      .to_string()
      .into(),
  );
  args.extend(["-pix_fmt", "yuv420p", "-profile:v", "high"].map(OsString::from));
  args.extend(audio_args.into_iter().map(OsString::from));
  args.extend(EXPORT_MP4_OUTPUT.map(OsString::from));
  args.push(destination.into());
  Ok(args)
}

#[allow(clippy::too_many_arguments)]
pub fn export_baked_recording(
  screen: &Path,
  camera: &Path,
  destination: &Path,
  selection: &TrackSelection,
  layout: AudioLayout,
  options: BakedVideoExportOptions,
  cancelled: &AtomicBool,
  on_progress: &mut dyn FnMut(u64),
) -> Result<ExportRunResult, String> {
  if !supports_compression() {
    return Err("This FFmpeg build does not include the H.264 encoder".to_owned());
  }
  let temporary = encode::remux_temp_path(destination);
  let args = baked_export_args(screen, camera, &temporary, selection, layout, options)?;
  encode::run_export(args, &temporary, destination, cancelled, on_progress)
}

pub type BakedRecordingExport = for<'a> fn(
  &Path,
  &Path,
  &Path,
  &TrackSelection,
  AudioLayout,
  BakedVideoExportOptions,
  &'a AtomicBool,
  &'a mut dyn FnMut(u64),
) -> Result<ExportRunResult, String>;

pub fn baked_recording_exporter() -> Option<BakedRecordingExport> {
  encode::ffmpeg_runs().then_some(export_baked_recording as BakedRecordingExport)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::exports::CameraOverlaySettings;

  #[test]
  fn builds_one_composed_video_and_keeps_selected_audio_separate() {
    let args = baked_export_args(
      Path::new("screen.mov"),
      Path::new("camera.mov"),
      Path::new("output.mp4"),
      &TrackSelection::default(),
      AudioLayout::SeparateTracks,
      BakedVideoExportOptions {
        camera_height: 1080,
        camera_width: 1920,
        overlay: CameraOverlaySettings {
          camera_width_percent: 25.0,
          camera_x_percent: 84.5,
          camera_y_percent: 11.0,
          frame_height_percent: 14.0,
          frame_width_percent: 25.0,
          frame_x_percent: 72.0,
          frame_y_percent: 4.0,
          radius_percent: 10.0,
        },
        screen_height: 2338,
        screen_width: 3600,
        video: VideoExportOptions {
          compression: 2,
          resolution_scale_percent: 100,
          source_scale_percent: 200,
        },
      },
    )
    .unwrap();
    let text = args
      .iter()
      .map(|arg| arg.to_string_lossy())
      .collect::<Vec<_>>()
      .join(" ");
    assert!(text.contains("scale=1800:1168"));
    assert!(text.contains("crop="));
    assert!(text.contains("overlay="));
    assert!(text.contains("eof_action=repeat"));
    assert!(text.contains("geq="));
    assert!(text.contains("-an"));
  }
}
