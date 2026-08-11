// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

fn main() {
  #[cfg(target_os = "macos")]
  {
    println!("cargo:rerun-if-changed=src/exports/cursor_export/gpu_compositor_macos.m");
    cc::Build::new()
      .file("src/exports/cursor_export/gpu_compositor_macos.m")
      .flag("-fobjc-arc")
      .compile("orbit_capture_gpu_compositor");
    for framework in [
      "AVFoundation",
      "CoreMedia",
      "CoreVideo",
      "Foundation",
      "Metal",
      "VideoToolbox",
    ] {
      println!("cargo:rustc-link-lib=framework={framework}");
    }
  }
  tauri_build::build()
}
