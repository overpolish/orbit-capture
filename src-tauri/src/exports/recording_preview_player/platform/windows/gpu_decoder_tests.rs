// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use windows::Win32::{
  Foundation::HMODULE,
  Graphics::{
    Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0},
    Direct3D10::ID3D10Multithread,
    Direct3D11::{
      D3D11CreateDevice, ID3D11Resource, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
      D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ,
      D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    },
    Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
  },
};

#[test]
#[ignore = "uses the video path in ORBIT_CAPTURE_WINDOWS_PREVIEW_TEST"]
fn decoded_preview_frame_stays_on_the_gpu() {
  let path = std::env::var_os("ORBIT_CAPTURE_WINDOWS_PREVIEW_TEST")
    .map(std::path::PathBuf::from)
    .expect("set ORBIT_CAPTURE_WINDOWS_PREVIEW_TEST to a recording");
  let mut device = None;
  unsafe {
    D3D11CreateDevice(
      None,
      D3D_DRIVER_TYPE_HARDWARE,
      HMODULE::default(),
      D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
      Some(&[D3D_FEATURE_LEVEL_11_0]),
      D3D11_SDK_VERSION,
      Some(&mut device),
      None,
      None,
    )
  }
  .unwrap();
  let device = device.expect("D3D11 returned no test device");
  let context = unsafe { device.GetImmediateContext() }.expect("D3D11 returned no test context");
  let multithread: ID3D10Multithread = device.cast().unwrap();
  let _ = unsafe { multithread.SetMultithreadProtected(true) };
  let mut reader = GpuVideoReader::open_with_device(&path, 0, device.clone()).unwrap();
  reader.seek(4_000).unwrap();
  let frame = reader.frame_at(4_000).unwrap().unwrap();
  let mut description = D3D11_TEXTURE2D_DESC::default();
  unsafe { frame.texture.GetDesc(&mut description) };
  let negotiated = reader.dimensions();
  assert!(frame.width >= negotiated.0 && frame.height >= negotiated.1);
  assert_eq!(description.Format, DXGI_FORMAT_B8G8R8A8_UNORM);

  let staging_description = D3D11_TEXTURE2D_DESC {
    Width: description.Width,
    Height: description.Height,
    MipLevels: 1,
    ArraySize: 1,
    Format: description.Format,
    SampleDesc: description.SampleDesc,
    Usage: D3D11_USAGE_STAGING,
    BindFlags: 0,
    CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
    MiscFlags: 0,
  };
  let mut staging = None;
  unsafe { device.CreateTexture2D(&staging_description, None, Some(&mut staging)) }.unwrap();
  let staging = staging.expect("D3D11 returned no staging texture");
  let source: ID3D11Resource = frame.texture.cast().unwrap();
  let target: ID3D11Resource = staging.cast().unwrap();
  unsafe { context.CopySubresourceRegion(&target, 0, 0, 0, 0, &source, frame.subresource, None) };
  let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
  unsafe { context.Map(&target, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) }.unwrap();
  let pixels = unsafe {
    std::slice::from_raw_parts(
      mapped.pData.cast::<u8>(),
      mapped.RowPitch as usize * description.Height as usize,
    )
  };
  let mut values = std::collections::BTreeSet::new();
  values.extend(pixels.iter().copied());
  unsafe { context.Unmap(&target, 0) };
  assert!(
    values.len() > 16,
    "decoded GPU texture is effectively uniform: {values:?}"
  );

  for target_ms in [5_000, 8_000, 12_000, 16_000, 18_000, 20_000, 22_000] {
    reader.seek(target_ms).unwrap();
    let frame = reader.frame_at(target_ms).unwrap().unwrap();
    println!(
      "requested {target_ms} ms, decoded {} ms",
      frame.timestamp_ms
    );
    assert!(
      frame.timestamp_ms.saturating_add(50) >= target_ms,
      "seek to {target_ms} ms stopped at {} ms",
      frame.timestamp_ms
    );
  }
}
