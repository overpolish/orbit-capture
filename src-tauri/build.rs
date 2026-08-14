// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

fn main() {
  #[cfg(windows)]
  if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
    compile_windows_preview_shaders();
  }
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

#[cfg(windows)]
fn compile_windows_preview_shaders() {
  use std::{ffi::CString, path::PathBuf};
  use windows::{
    core::PCSTR,
    Win32::Graphics::Direct3D::{Fxc::D3DCompile, ID3DBlob},
  };

  const SOURCE_PATH: &str = "src/exports/preview_platform/surface_windows/compositor.rs";
  const START: &str = "const SHADER: &str = r#\"";
  const END: &str = "\"#;";
  println!("cargo:rerun-if-changed={SOURCE_PATH}");
  let rust = std::fs::read_to_string(SOURCE_PATH).expect("read the Windows preview shader");
  let start = rust.find(START).expect("find the preview shader start") + START.len();
  let end = rust[start..]
    .find(END)
    .map(|offset| start + offset)
    .expect("find the preview shader end");
  let source = &rust.as_bytes()[start..end];
  let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo supplied OUT_DIR"));

  for (entry, target, filename) in [
    ("vs_main", "vs_4_0", "recording_preview_vs.cso"),
    ("ps_main", "ps_4_0", "recording_preview_ps.cso"),
  ] {
    let entry = CString::new(entry).expect("valid shader entry");
    let target = CString::new(target).expect("valid shader target");
    let mut code: Option<ID3DBlob> = None;
    let mut errors: Option<ID3DBlob> = None;
    let result = unsafe {
      D3DCompile(
        source.as_ptr().cast(),
        source.len(),
        PCSTR::null(),
        None,
        None,
        PCSTR(entry.as_ptr().cast()),
        PCSTR(target.as_ptr().cast()),
        0,
        0,
        &mut code,
        Some(&mut errors),
      )
    };
    if let Err(error) = result {
      let detail = errors.map_or_else(String::new, |blob| unsafe {
        let bytes =
          std::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize());
        String::from_utf8_lossy(bytes)
          .trim_matches(char::from(0))
          .to_owned()
      });
      panic!("Windows preview shader compilation failed: {error}: {detail}");
    }
    let code = code.expect("D3DCompile returned preview bytecode");
    let bytes = unsafe {
      std::slice::from_raw_parts(code.GetBufferPointer().cast::<u8>(), code.GetBufferSize())
    };
    std::fs::write(output.join(filename), bytes).expect("write compiled preview shader");
  }
}
