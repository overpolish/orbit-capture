// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

fn main() {
  // Build scripts are compiled for the host, so `cfg!(target_os)` here answers
  // "what am I running on", not "what am I building for". Cross-compiling from
  // macOS to Windows must not hand the Objective-C sources to the MSVC target.
  if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
    println!("cargo:rerun-if-changed=src/exports/cursor_export/gpu_compositor_macos.m");
    println!("cargo:rerun-if-changed=src/exports/recording_preview_surface_macos.m");
    println!("cargo:rerun-if-changed=src/exports/cursor_export/gpu_compositor_macos.h");
    println!("cargo:rerun-if-changed=src/exports/recording_preview_reader_macos.m");
    println!("cargo:rerun-if-changed=src/exports/recording_preview_scrubber_macos.m");
    cc::Build::new()
      .file("src/exports/cursor_export/gpu_compositor_macos.m")
      .file("src/exports/recording_preview_reader_macos.m")
      .file("src/exports/recording_preview_scrubber_macos.m")
      .file("src/exports/recording_preview_surface_macos.m")
      .flag("-fobjc-arc")
      .compile("orbit_capture_gpu_compositor");
    for framework in [
      "AVFoundation",
      "AppKit",
      "CoreMedia",
      "CoreVideo",
      "Foundation",
      "Metal",
      "QuartzCore",
      "VideoToolbox",
    ] {
      println!("cargo:rustc-link-lib=framework={framework}");
    }
  }
  tauri_build::build()
}
