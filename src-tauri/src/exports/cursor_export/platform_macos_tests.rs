// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::{
  exports::CameraOverlaySettings,
  recording::cursor::{CursorRecord, CursorSource, CursorSourceKind, CursorStyle, FORMAT_VERSION},
};
use std::process::Command;

#[test]
fn exports_composited_cursor_pixels_into_a_real_movie() {
  let directory =
    std::env::temp_dir().join(format!("orbit-cursor-export-test-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&directory);
  std::fs::create_dir_all(&directory).unwrap();
  let source = directory.join("source.mov");
  let cursor_path = directory.join("source.cursor.jsonl");
  let destination = directory.join("output.mp4");
  let status = Command::new(media_preview::ffmpeg_path())
    .args([
      "-hide_banner",
      "-loglevel",
      "error",
      "-y",
      "-f",
      "lavfi",
      "-i",
      "color=c=black:s=320x180:r=30:d=1",
      "-c:v",
      "libx264",
      "-pix_fmt",
      "yuv420p",
    ])
    .arg(&source)
    .status()
    .unwrap();
  assert!(status.success());

  let records = [
    CursorRecord::Header {
      coordinate_space: "global-logical-points".to_owned(),
      platform: "macos".to_owned(),
      source: CursorSource {
        height: 180.0,
        kind: CursorSourceKind::Screen,
        platform_id: "test".to_owned(),
        video_height: 180,
        video_width: 320,
        width: 320.0,
        x: 0.0,
        y: 0.0,
      },
      timebase: "recording-microseconds".to_owned(),
      version: FORMAT_VERSION,
    },
    CursorRecord::Appearance {
      height: 24.0,
      hotspot_x: 1.0,
      hotspot_y: 1.0,
      style: CursorStyle::Arrow,
      timestamp_us: 0,
      width: 16.0,
    },
    CursorRecord::Position {
      timestamp_us: 0,
      x: 80.0,
      y: 80.0,
    },
    CursorRecord::Position {
      timestamp_us: 1_000_000,
      x: 240.0,
      y: 80.0,
    },
  ];
  let json = records
    .iter()
    .map(|record| serde_json::to_string(record).unwrap())
    .collect::<Vec<_>>()
    .join("\n");
  std::fs::write(&cursor_path, format!("{json}\n")).unwrap();

  let cancelled = AtomicBool::new(false);
  let mut progress = Vec::new();
  let result = export(CursorExportRequest {
    audio_layout: AudioLayout::SeparateTracks,
    camera: None,
    cancelled: &cancelled,
    cursor: &cursor_path,
    cursor_effects: CursorEffectSettings::default(),
    destination: &destination,
    duration_ms: 1_000,
    height: 180,
    on_progress: &mut |position| progress.push(position),
    screen: &source,
    selection: &TrackSelection::default(),
    video: VideoExportOptions {
      compression: 1,
      resolution_scale_percent: 50,
      source_scale_percent: 100,
    },
    width: 320,
  })
  .unwrap();
  assert_eq!(result, ExportRunResult::Completed);
  assert!(destination.is_file());
  assert!(progress.last().is_some_and(|position| *position > 900));
  let metadata = Command::new(media_preview::ffmpeg_path())
    .args(["-hide_banner", "-nostdin", "-i"])
    .arg(&destination)
    .output()
    .unwrap();
  assert!(
    String::from_utf8_lossy(&metadata.stderr).contains("Video: h264"),
    "the delivered cursor-baked recording must use compatible H.264 video"
  );

  for timestamp in ["0", "0.5"] {
    let frame = Command::new(media_preview::ffmpeg_path())
      .args(["-hide_banner", "-loglevel", "error", "-ss", timestamp, "-i"])
      .arg(&destination)
      .args([
        "-frames:v",
        "1",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgb24",
        "pipe:1",
      ])
      .output()
      .unwrap();
    assert!(frame.status.success());
    assert_eq!(frame.stdout.len(), 160 * 90 * 3);
    assert!(
      frame.stdout.iter().any(|channel| *channel > 200),
      "the frame at {timestamp}s should contain the cursor"
    );
  }
  let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn exports_camera_and_cursor_through_the_same_gpu_compositor() {
  let directory =
    std::env::temp_dir().join(format!("orbit-camera-cursor-test-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&directory);
  std::fs::create_dir_all(&directory).unwrap();
  let source = directory.join("source.mov");
  let camera = directory.join("camera.mov");
  let cursor_path = directory.join("source.cursor.jsonl");
  let destination = directory.join("output.mp4");
  for (path, color, size) in [(&source, "black", "320x180"), (&camera, "red", "160x120")] {
    let status = Command::new(media_preview::ffmpeg_path())
      .args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "lavfi",
        "-i",
      ])
      .arg(format!("color=c={color}:s={size}:r=30:d=1"))
      .args(["-c:v", "libx264", "-pix_fmt", "yuv420p"])
      .arg(path)
      .status()
      .unwrap();
    assert!(status.success());
  }
  let records = [
    CursorRecord::Header {
      coordinate_space: "global-logical-points".to_owned(),
      platform: "macos".to_owned(),
      source: CursorSource {
        height: 180.0,
        kind: CursorSourceKind::Screen,
        platform_id: "test".to_owned(),
        video_height: 180,
        video_width: 320,
        width: 320.0,
        x: 0.0,
        y: 0.0,
      },
      timebase: "recording-microseconds".to_owned(),
      version: FORMAT_VERSION,
    },
    CursorRecord::Appearance {
      height: 24.0,
      hotspot_x: 1.0,
      hotspot_y: 1.0,
      style: CursorStyle::Arrow,
      timestamp_us: 0,
      width: 16.0,
    },
    CursorRecord::Position {
      timestamp_us: 0,
      x: 80.0,
      y: 60.0,
    },
  ];
  let json = records
    .iter()
    .map(|record| serde_json::to_string(record).unwrap())
    .collect::<Vec<_>>()
    .join("\n");
  std::fs::write(&cursor_path, format!("{json}\n")).unwrap();

  let cancelled = AtomicBool::new(false);
  let result = export(CursorExportRequest {
    audio_layout: AudioLayout::SeparateTracks,
    camera: Some((
      &camera,
      BakedVideoExportOptions {
        camera_height: 120,
        camera_width: 160,
        overlay: CameraOverlaySettings {
          camera_width_percent: 25.0,
          camera_x_percent: 31.25,
          camera_y_percent: 38.888_89,
          frame_height_percent: 33.333_33,
          frame_width_percent: 25.0,
          frame_x_percent: 18.75,
          frame_y_percent: 22.222_22,
          radius_percent: 10.0,
        },
        screen_height: 180,
        screen_width: 320,
        video: VideoExportOptions {
          compression: 1,
          resolution_scale_percent: 100,
          source_scale_percent: 100,
        },
      },
    )),
    cancelled: &cancelled,
    cursor: &cursor_path,
    cursor_effects: CursorEffectSettings::default(),
    destination: &destination,
    duration_ms: 1_000,
    height: 180,
    on_progress: &mut |_| {},
    screen: &source,
    selection: &TrackSelection::default(),
    video: VideoExportOptions {
      compression: 1,
      resolution_scale_percent: 100,
      source_scale_percent: 100,
    },
    width: 320,
  })
  .unwrap();
  assert_eq!(result, ExportRunResult::Completed);
  let frame = Command::new(media_preview::ffmpeg_path())
    .args(["-hide_banner", "-loglevel", "error", "-ss", "0.5", "-i"])
    .arg(&destination)
    .args([
      "-frames:v",
      "1",
      "-f",
      "rawvideo",
      "-pix_fmt",
      "rgb24",
      "pipe:1",
    ])
    .output()
    .unwrap();
  assert!(frame.status.success());
  assert!(frame
    .stdout
    .chunks_exact(3)
    .any(|pixel| pixel[0] > 180 && pixel[1] < 80 && pixel[2] < 80));
  assert!(frame
    .stdout
    .chunks_exact(3)
    .enumerate()
    .filter(|(index, _)| {
      let x = index % 320;
      let y = index / 320;
      (65..135).contains(&x) && (45..95).contains(&y)
    })
    .all(|(_, pixel)| !(pixel[0] > 180 && pixel[1] > 180 && pixel[2] > 180)));
  let _ = std::fs::remove_dir_all(directory);
}

#[test]
#[ignore = "set ORBIT_GPU_BENCH_SOURCE to a 3600 x 2338 recording"]
fn benchmarks_retina_gpu_cursor_export() {
  let source = PathBuf::from(std::env::var("ORBIT_GPU_BENCH_SOURCE").unwrap());
  let duration_ms = std::env::var("ORBIT_GPU_BENCH_DURATION_MS")
    .ok()
    .and_then(|value| value.parse().ok())
    .unwrap_or(40_908);
  let resolution_scale_percent = std::env::var("ORBIT_GPU_BENCH_SCALE_PERCENT")
    .ok()
    .and_then(|value| value.parse().ok())
    .unwrap_or(100);
  let directory =
    std::env::temp_dir().join(format!("orbit-gpu-export-benchmark-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&directory);
  std::fs::create_dir_all(&directory).unwrap();
  let cursor_path = directory.join("source.cursor.jsonl");
  let destination = directory.join("output.mp4");
  let records = [
    CursorRecord::Header {
      coordinate_space: "global-logical-points".to_owned(),
      platform: "macos".to_owned(),
      source: CursorSource {
        height: 1_169.0,
        kind: CursorSourceKind::Screen,
        platform_id: "benchmark".to_owned(),
        video_height: 2_338,
        video_width: 3_600,
        width: 1_800.0,
        x: 0.0,
        y: 0.0,
      },
      timebase: "recording-microseconds".to_owned(),
      version: FORMAT_VERSION,
    },
    CursorRecord::Appearance {
      height: 24.0,
      hotspot_x: 1.0,
      hotspot_y: 1.0,
      style: CursorStyle::Arrow,
      timestamp_us: 0,
      width: 16.0,
    },
    CursorRecord::Position {
      timestamp_us: 0,
      x: 300.0,
      y: 300.0,
    },
    CursorRecord::Position {
      timestamp_us: duration_ms * 1_000,
      x: 1_500.0,
      y: 800.0,
    },
  ];
  let json = records
    .iter()
    .map(|record| serde_json::to_string(record).unwrap())
    .collect::<Vec<_>>()
    .join("\n");
  std::fs::write(&cursor_path, format!("{json}\n")).unwrap();
  let cancelled = AtomicBool::new(false);
  let started = std::time::Instant::now();
  let result = export(CursorExportRequest {
    audio_layout: AudioLayout::SeparateTracks,
    camera: None,
    cancelled: &cancelled,
    cursor: &cursor_path,
    cursor_effects: CursorEffectSettings::default(),
    destination: &destination,
    duration_ms,
    height: 2_338,
    on_progress: &mut |_| {},
    screen: &source,
    selection: &TrackSelection::default(),
    video: VideoExportOptions {
      compression: 2,
      resolution_scale_percent,
      source_scale_percent: 100,
    },
    width: 3_600,
  })
  .unwrap();
  assert_eq!(result, ExportRunResult::Completed);
  eprintln!(
    "[cursor-export-benchmark] exported {:.2}s in {:.2}s to {}",
    duration_ms as f64 / 1_000.0,
    started.elapsed().as_secs_f64(),
    destination.display()
  );
}
