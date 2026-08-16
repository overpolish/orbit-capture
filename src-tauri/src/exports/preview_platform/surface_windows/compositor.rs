// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! One-pass D3D11 preview compositor. Decoded frames remain on the shared GPU;
//! the CPU only updates this pass's small constant buffer.

use std::{ffi::c_void, path::PathBuf};

use windows::{
  core::{w, Interface, PCWSTR},
  Win32::Foundation::ERROR_SUCCESS,
  Win32::Graphics::{
    Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
    Direct3D11::{
      ID3D11BlendState, ID3D11Buffer, ID3D11Device, ID3D11DeviceContext, ID3D11PixelShader,
      ID3D11RenderTargetView, ID3D11Resource, ID3D11SamplerState, ID3D11ShaderResourceView,
      ID3D11Texture2D, ID3D11VertexShader, D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_SHADER_RESOURCE,
      D3D11_BLEND_DESC, D3D11_BLEND_INV_SRC_ALPHA, D3D11_BLEND_ONE, D3D11_BLEND_OP_ADD,
      D3D11_BUFFER_DESC, D3D11_COLOR_WRITE_ENABLE_ALL, D3D11_FILTER_MIN_MAG_MIP_LINEAR,
      D3D11_RENDER_TARGET_BLEND_DESC, D3D11_SAMPLER_DESC, D3D11_SUBRESOURCE_DATA,
      D3D11_TEXTURE2D_DESC, D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_USAGE_DEFAULT, D3D11_VIEWPORT,
    },
    Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC},
    Gdi::{
      CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
      BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    },
  },
  Win32::System::{
    Environment::ExpandEnvironmentStringsW,
    Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ, RRF_ZEROONFAILURE},
  },
  Win32::UI::WindowsAndMessaging::{
    DestroyCursor, DrawIconEx, GetIconInfo, LoadImageW, DI_NORMAL, HCURSOR, IDC_ARROW, IDC_CROSS,
    IDC_HAND, IDC_IBEAM, IDC_NO, IDC_SIZENS, IDC_SIZEWE, IMAGE_CURSOR, LR_LOADFROMFILE, LR_SHARED,
  },
};

use crate::exports::media_preview::BakeGeometry;
use crate::screenshots::{
  output_placement, parse_hex_colour, validate_mesh, ScreenshotOutputSettings,
};

const SHADER: &str = r#"
cbuffer Canvas : register(b0) {
  float4 output_source; // output width/height, source width/height
  float4 image_rect;
  float4 crop_rect;
  float4 solid_color;
  float4 base_color;
  float4 mesh_points[8];
  float4 mesh_colors[4];
  float4 effects; // image radius, background radius, warp, shadow sigma
  float4 motion; // timeline seconds
  float4 cursor_geometry; // source-space anchor x/y, artwork width/height
  float4 cursor_effects; // reserved x/y, rotation radians, scale
  float4 cursor_blur; // source-space frame delta x/y
  float4 camera_frame; // output-space x/y/width/height
  float4 camera_crop; // camera source-space x/y/width/height
  float4 camera_effects; // radius, enabled, shadow sigma, camera on top
  float4 native_cursor_hotspots[8]; // normalized atlas hotspot x/y
  uint4 options; // seed, mesh enabled, point count, shadow enabled
  uint4 cursor_options; // artwork, enabled, clip to video, foreground only
};
Texture2D source_image : register(t0);
Texture2DArray native_cursor_images : register(t1);
Texture2D camera_image : register(t2);
SamplerState linear_sampler : register(s0);

float hash(float2 position, uint seed) {
  float value = sin(dot(position, float2(127.1, 311.7)) + (float)seed * 0.017) * 43758.5453;
  return frac(value) * 2.0 - 1.0;
}

float noise(float2 position, uint seed) {
  float2 cell = floor(position), local = frac(position);
  float2 eased = local * local * (3.0 - 2.0 * local);
  float top = lerp(hash(cell, seed), hash(cell + float2(1, 0), seed), eased.x);
  float bottom = lerp(hash(cell + float2(0, 1), seed), hash(cell + 1, seed), eased.x);
  return lerp(top, bottom, eased.y);
}
float fractal_noise(float2 position, uint seed) {
  return noise(position, seed) * 0.58
    + noise(position * 2.07 + float2(11.3, -4.9), seed ^ 0x68bc21eb) * 0.28
    + noise(position * 4.19 + float2(-8.7, 13.1), seed ^ 0x02e5be93) * 0.14;
}
float rounded_distance(float2 pixel, float4 rect, float radius) {
  float2 half_size = rect.zw * 0.5;
  float2 local = abs(pixel - (rect.xy + half_size)) - (half_size - radius);
  return length(max(local, 0.0)) + min(max(local.x, local.y), 0.0) - radius;
}
float rounded_coverage(float2 pixel, float4 rect, float radius) {
  float distance = rounded_distance(pixel, rect, radius);
  if (radius <= 0.0) return distance < 0.0 ? 1.0 : 0.0;
  return 1.0 - smoothstep(-0.75, 0.75, distance);
}
float visible_shadow(float2 pixel, float sigma) {
  float2 shadow_pixel = pixel - float2(0, sigma * 0.35);
  // The foreground is the intersection of the crop window and placed source.
  // A tall crop around a wide source must not cast a tall rectangular shadow.
  float crop_distance = rounded_distance(shadow_pixel, crop_rect, effects.x);
  float image_distance = rounded_distance(shadow_pixel, image_rect, 0.0);
  float distance = max(max(crop_distance, image_distance), 0.0);
  return (36.0 / 255.0) * exp(-0.5 * distance * distance / (sigma * sigma));
}
float4 cursor_sample(float2 source_pixel, float2 anchor) {
  // The D3D preview's screen-space rotation convention is opposite to the
  // raster/Metal convention used to calculate the motion lean.
  float angle = -cursor_effects.z;
  if (cursor_options.x == 2) angle += 1.57079632679;
  float2 delta = source_pixel - anchor;
  float cosine = cos(angle), sine = sin(angle);
  float2 local = float2(cosine * delta.x + sine * delta.y,
                        -sine * delta.x + cosine * delta.y) / max(cursor_effects.w, 0.01);
  float2 atlas_uv = local / cursor_geometry.zw + native_cursor_hotspots[cursor_options.x].xy;
  if (any(atlas_uv < 0.0) || any(atlas_uv >= 1.0)) return 0.0;
  return native_cursor_images.Sample(linear_sampler, float3(atlas_uv, (float)cursor_options.x));
}
float4 cursor_layer(float2 pixel) {
  if (cursor_options.y == 0) return 0.0;
  float2 source_pixel = (pixel - image_rect.xy) / image_rect.zw * output_source.zw;
  float travel = min(length(cursor_blur.xy), 80.0);
  float radius = length(cursor_geometry.zw) * cursor_effects.w + travel + 4.0;
  if (any(abs(source_pixel - cursor_geometry.xy) > radius)) return 0.0;
  if (travel <= 1.25) return cursor_sample(source_pixel, cursor_geometry.xy);
  float2 direction = cursor_blur.xy / max(length(cursor_blur.xy), 0.001);
  float4 accumulated = 0.0;
  float total_weight = 0.0;
  [unroll] for (uint index = 0; index < 24; ++index) {
    float progress = (float)index / 23.0;
    float centered = (progress - 0.5) / 0.34;
    float weight = exp(-0.5 * centered * centered);
    float2 sample_anchor = cursor_geometry.xy + direction * ((progress - 0.8) * travel);
    float4 sample = cursor_sample(source_pixel, sample_anchor);
    accumulated.rgb += sample.rgb * sample.a * weight;
    accumulated.a += sample.a * weight;
    total_weight += weight;
  }
  accumulated.a /= total_weight;
  accumulated.rgb = accumulated.a > 0.0 ? accumulated.rgb / (total_weight * accumulated.a) : 0.0;
  return accumulated;
}
float3 background(float2 pixel) {
  if (options.y == 0) return solid_color.rgb;
  float shortest = min(output_source.x, output_source.y);
  float2 dimensions = output_source.xy;
  float2 aspect = dimensions / shortest;
  float frequency = 3.5 / shortest;
  float phase = motion.x * 0.28;
  float2 drift = float2(sin(phase), cos(phase * 0.83)) * shortest * 0.012;
  float2 warped_pixel = pixel + drift;
  float warp_scale = shortest * effects.z / 100.0;
  float2 warp = float2(
    fractal_noise(warped_pixel * frequency + phase * 0.035, options.x),
    fractal_noise(warped_pixel * frequency + float2(19.7, -7.3) - phase * 0.03, options.x ^ 0xa511e9b3)
  ) * warp_scale;
  float3 weighted = base_color.rgb * 0.18;
  float total = 0.18;
  [loop] for (uint index = 0; index < options.z; ++index) {
    float4 first = mesh_points[index * 2];
    float4 second = mesh_points[index * 2 + 1];
    float local_phase = phase + (float)index * 1.73;
    float2 animated_center = first.xy + float2(sin(local_phase), cos(local_phase * 0.91)) * 0.012;
    float2 delta = (pixel + warp) / shortest - animated_center * aspect;
    float2 rotated = float2(delta.x * second.x + delta.y * second.y,
                            -delta.x * second.y + delta.y * second.x);
    float distance = length(rotated / max(first.zw, 0.01));
    float weight = 1.0 / (pow(max(distance, 0.025), 3.5) + 0.012);
    weighted += mesh_colors[index].rgb * weight;
    total += weight;
  }
  float3 result = weighted / total;
  float depth = fractal_noise((pixel + drift) * frequency * 0.7, options.x ^ 0xd1b54a35) * 13.0 / 255.0;
  return saturate(result + depth);
}
float4 vs_main(uint id : SV_VertexID) : SV_Position {
  float2 position = float2((id << 1) & 2, id & 2);
  return float4(position * float2(2, -2) + float2(-1, 1), 0, 1);
}
float4 camera_layer(float4 result, float2 pixel) {
  if (camera_effects.y == 0.0) return result;
  float camera_alpha = rounded_coverage(pixel, camera_frame, camera_effects.x);
  if (camera_effects.z > 1.0) {
    float2 shadow_pixel = pixel - float2(0, camera_effects.z * 0.35);
    float distance = max(rounded_distance(
      shadow_pixel, camera_frame, camera_effects.x), 0.0);
    float shadow = (36.0 / 255.0) * exp(
      -0.5 * distance * distance / (camera_effects.z * camera_effects.z));
    result.rgb *= 1.0 - shadow * (1.0 - camera_alpha);
  }
  float2 camera_local = (pixel - camera_frame.xy) / camera_frame.zw;
  if (camera_alpha > 0.0 && all(camera_local >= 0.0) && all(camera_local <= 1.0)) {
    float2 camera_source_pixel = camera_crop.xy + camera_local * camera_crop.zw;
    uint camera_width, camera_height;
    camera_image.GetDimensions(camera_width, camera_height);
    float2 camera_uv = camera_source_pixel / float2(camera_width, camera_height);
    float4 camera = camera_image.Sample(linear_sampler, camera_uv);
    result.rgb = lerp(result.rgb, camera.rgb, camera.a * camera_alpha);
    result.a = camera.a * camera_alpha + result.a * (1.0 - camera.a * camera_alpha);
  }
  return result;
}
float4 ps_main(float4 position : SV_Position) : SV_Target {
  float2 pixel = position.xy;
  float background_alpha = rounded_coverage(pixel, float4(0, 0, output_source.xy), effects.y);
  // Compose the complete frame first. Canvas rounding is a final output mask,
  // not merely a property of the background layer.
  float4 result = cursor_options.w != 0
    ? float4(0.0, 0.0, 0.0, 0.0)
    : float4(background(pixel), 1.0);
  if (camera_effects.w == 0.0) result = camera_layer(result, pixel);
  float crop_alpha = rounded_coverage(pixel, crop_rect, effects.x);
  // Axis-aligned zero-radius source edges are already pixel-exact. Smoothing
  // them leaks a fractional row of canvas colour around a default crop.
  float image_rect_alpha = rounded_coverage(pixel, image_rect, 0.0);
  float image_alpha = crop_alpha * image_rect_alpha;
  if (options.w != 0 && effects.w > 1.0) {
    float shadow = visible_shadow(pixel, effects.w);
    if (cursor_options.w != 0) {
      result.a = shadow * (1.0 - image_alpha);
    } else {
      result.rgb *= 1.0 - shadow * (1.0 - image_alpha);
    }
  }
  float2 uv = (pixel - image_rect.xy) / image_rect.zw;
  if (image_alpha > 0.0) {
    float4 video = source_image.Sample(linear_sampler, uv);
    result = lerp(result, float4(video.rgb, 1.0), video.a * image_alpha);
  }
  float4 cursor = cursor_layer(pixel);
  if (cursor_options.z != 0) cursor.a *= image_alpha;
  result.rgb = lerp(result.rgb, cursor.rgb, cursor.a);
  result.a = cursor.a + result.a * (1.0 - cursor.a);
  if (camera_effects.w != 0.0) result = camera_layer(result, pixel);
  // Dither belongs to the completed stack, applied once by the base layer. A
  // foreground layer must keep its uncovered pixels at exactly zero or the
  // lifted colour turns invalid premultiplied alpha and brightens the layers
  // beneath it.
  if (cursor_options.w == 0) {
    result.rgb = saturate(result.rgb + hash(pixel, 0x9e3779b9) / 255.0);
  }
  // DirectComposition consumes premultiplied alpha. Clip every composed layer
  // to the rounded canvas and premultiply only once at the final boundary.
  result *= background_alpha;
  return result;
}
"#;

const VERTEX_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/recording_preview_vs.cso"));
const PIXEL_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/recording_preview_ps.cso"));

#[repr(C)]
#[derive(Clone, Copy)]
struct Constants {
  output_source: [f32; 4],
  image_rect: [f32; 4],
  crop_rect: [f32; 4],
  solid_color: [f32; 4],
  base_color: [f32; 4],
  mesh_points: [[f32; 4]; 8],
  mesh_colors: [[f32; 4]; 4],
  effects: [f32; 4],
  motion: [f32; 4],
  cursor_geometry: [f32; 4],
  cursor_effects: [f32; 4],
  cursor_blur: [f32; 4],
  camera_frame: [f32; 4],
  camera_crop: [f32; 4],
  camera_effects: [f32; 4],
  native_cursor_hotspots: [[f32; 4]; 8],
  options: [u32; 4],
  cursor_options: [u32; 4],
}

pub(super) struct Compositor {
  constants: ID3D11Buffer,
  cursor_hotspots: [[f32; 4]; 8],
  cursor_view: ID3D11ShaderResourceView,
  layer_blend: ID3D11BlendState,
  pixel_shader: ID3D11PixelShader,
  sampler: ID3D11SamplerState,
  vertex_shader: ID3D11VertexShader,
}

#[derive(Clone)]
pub(super) struct SourceTexture {
  pub(super) size: (u32, u32),
  texture: ID3D11Texture2D,
  view: ID3D11ShaderResourceView,
}

fn color(value: &str) -> Result<[f32; 4], String> {
  Ok(parse_hex_colour(value)?.map(|channel| f32::from(channel) / 255.0))
}

fn cursor_scheme_path(value_name: &str, fallback_name: &str) -> Option<PathBuf> {
  let value_name = value_name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
  let flags = RRF_RT_REG_SZ | RRF_ZEROONFAILURE;
  let mut byte_length = 0;
  if unsafe {
    RegGetValueW(
      HKEY_CURRENT_USER,
      w!("Control Panel\\Cursors"),
      PCWSTR(value_name.as_ptr()),
      flags,
      None,
      None,
      Some(&mut byte_length),
    )
  } != ERROR_SUCCESS
  {
    return None;
  }
  let mut value = vec![0_u16; (byte_length as usize).div_ceil(2).max(1)];
  if unsafe {
    RegGetValueW(
      HKEY_CURRENT_USER,
      w!("Control Panel\\Cursors"),
      PCWSTR(value_name.as_ptr()),
      flags,
      None,
      Some(value.as_mut_ptr().cast()),
      Some(&mut byte_length),
    )
  } != ERROR_SUCCESS
  {
    return None;
  }
  let end = value
    .iter()
    .position(|character| *character == 0)
    .unwrap_or(value.len());
  value.truncate(end);
  if value.is_empty() {
    value = std::env::var_os("WINDIR")?
      .to_string_lossy()
      .encode_utf16()
      .chain("\\Cursors\\".encode_utf16())
      .chain(fallback_name.encode_utf16())
      .collect();
  }
  value.push(0);
  let expanded_length = unsafe { ExpandEnvironmentStringsW(PCWSTR(value.as_ptr()), None) };
  if expanded_length == 0 {
    return None;
  }
  let mut expanded = vec![0_u16; expanded_length as usize];
  let written =
    unsafe { ExpandEnvironmentStringsW(PCWSTR(value.as_ptr()), Some(expanded.as_mut_slice())) };
  if written == 0 || written > expanded_length {
    return None;
  }
  expanded.truncate(written.saturating_sub(1) as usize);
  Some(PathBuf::from(String::from_utf16_lossy(&expanded)))
}

fn native_cursor_pixels(
  cursor_name: PCWSTR,
  scheme_value: &str,
  fallback_file: &str,
) -> Result<(u32, u32, [f32; 2], Vec<u8>), String> {
  // Cursor effects can enlarge artwork to 500%. Asking user32 for a large
  // cursor makes it select the highest-resolution image embedded in the
  // active Windows cursor resource instead of permanently baking the atlas
  // from the nominal 32 px representation.
  const ATLAS_CURSOR_SIZE: u32 = 128;
  let width = ATLAS_CURSOR_SIZE;
  let height = ATLAS_CURSOR_SIZE;
  let scheme_path = cursor_scheme_path(scheme_value, fallback_file);
  let scheme_path_wide = scheme_path.as_ref().map(|path| {
    path
      .as_os_str()
      .to_string_lossy()
      .encode_utf16()
      .chain(Some(0))
      .collect::<Vec<_>>()
  });
  let file_cursor = scheme_path_wide.as_ref().and_then(|path| {
    unsafe {
      LoadImageW(
        None,
        PCWSTR(path.as_ptr()),
        IMAGE_CURSOR,
        width as i32,
        height as i32,
        LR_LOADFROMFILE,
      )
    }
    .ok()
  });
  let (cursor, owned) = if let Some(cursor) = file_cursor {
    (HCURSOR(cursor.0), true)
  } else {
    let cursor = unsafe {
      LoadImageW(
        None,
        cursor_name,
        IMAGE_CURSOR,
        width as i32,
        height as i32,
        LR_SHARED,
      )
    }
    .map_err(|error| error.to_string())?;
    (HCURSOR(cursor.0), false)
  };
  let mut cursor_info = windows::Win32::UI::WindowsAndMessaging::ICONINFO::default();
  if let Err(error) = unsafe { GetIconInfo(cursor.into(), &mut cursor_info) } {
    if owned {
      let _ = unsafe { DestroyCursor(cursor) };
    }
    return Err(error.to_string());
  }
  let hotspot = [
    (cursor_info.xHotspot as f32 + 0.5) / width as f32,
    (cursor_info.yHotspot as f32 + 0.5) / height as f32,
  ];
  if !cursor_info.hbmColor.is_invalid() {
    let _ = unsafe { DeleteObject(cursor_info.hbmColor.into()) };
  }
  if !cursor_info.hbmMask.is_invalid() {
    let _ = unsafe { DeleteObject(cursor_info.hbmMask.into()) };
  }
  let render = |background: u8| -> Result<Vec<u8>, String> {
    let info = BITMAPINFO {
      bmiHeader: BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width as i32,
        biHeight: -(height as i32),
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
      },
      ..Default::default()
    };
    let dc = unsafe { CreateCompatibleDC(None) };
    if dc.is_invalid() {
      return Err("Windows could not create a cursor drawing context".to_owned());
    }
    let mut bits = std::ptr::null_mut();
    let bitmap = unsafe {
      CreateDIBSection(
        Some(dc),
        &raw const info,
        DIB_RGB_COLORS,
        &mut bits,
        None,
        0,
      )
    }
    .map_err(|error| {
      let _ = unsafe { DeleteDC(dc) };
      error.to_string()
    })?;
    let old = unsafe { SelectObject(dc, bitmap.into()) };
    let length = (width * height * 4) as usize;
    let pixels = unsafe { std::slice::from_raw_parts_mut(bits.cast::<u8>(), length) };
    for pixel in pixels.chunks_exact_mut(4) {
      pixel.fill(background);
      pixel[3] = 255;
    }
    let drawn = unsafe {
      DrawIconEx(
        dc,
        0,
        0,
        cursor.into(),
        width as i32,
        height as i32,
        0,
        None,
        DI_NORMAL,
      )
    };
    let result = drawn
      .map(|()| pixels.to_vec())
      .map_err(|error| error.to_string());
    unsafe {
      SelectObject(dc, old);
      let _ = DeleteObject(bitmap.into());
      let _ = DeleteDC(dc);
    }
    result
  };
  let black = render(0);
  let white = render(255);
  if owned {
    let _ = unsafe { DestroyCursor(cursor) };
  }
  let black = black?;
  let white = white?;
  let mut pixels = vec![0_u8; black.len()];
  for ((output, black), white) in pixels
    .chunks_exact_mut(4)
    .zip(black.chunks_exact(4))
    .zip(white.chunks_exact(4))
  {
    let background = (0..3)
      .map(|channel| i32::from(white[channel]) - i32::from(black[channel]))
      .sum::<i32>()
      / 3;
    let alpha = (255 - background).clamp(0, 255) as u8;
    output[3] = alpha;
    if alpha != 0 {
      for channel in 0..3 {
        output[channel] = ((u32::from(black[channel]) * 255 + u32::from(alpha) / 2)
          / u32::from(alpha))
        .min(255) as u8;
      }
    }
  }
  Ok((width, height, hotspot, pixels))
}

impl Compositor {
  pub(super) fn new(device: &ID3D11Device) -> Result<Self, String> {
    debug_assert!(!SHADER.is_empty());
    let mut vertex_shader = None;
    let mut pixel_shader = None;
    unsafe {
      device
        .CreateVertexShader(VERTEX_SHADER, None, Some(&mut vertex_shader))
        .map_err(|error| error.to_string())?;
      device
        .CreatePixelShader(PIXEL_SHADER, None, Some(&mut pixel_shader))
        .map_err(|error| error.to_string())?;
    }
    let description = D3D11_BUFFER_DESC {
      ByteWidth: size_of::<Constants>() as u32,
      Usage: D3D11_USAGE_DEFAULT,
      BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
      ..Default::default()
    };
    let mut constants = None;
    unsafe { device.CreateBuffer(&description, None, Some(&mut constants)) }
      .map_err(|error| error.to_string())?;
    let sampler_description = D3D11_SAMPLER_DESC {
      Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
      AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
      AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
      AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
      MaxLOD: f32::MAX,
      ..Default::default()
    };
    let mut sampler = None;
    unsafe { device.CreateSamplerState(&sampler_description, Some(&mut sampler)) }
      .map_err(|error| error.to_string())?;
    let layer_target = D3D11_RENDER_TARGET_BLEND_DESC {
      BlendEnable: true.into(),
      SrcBlend: D3D11_BLEND_ONE,
      DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
      BlendOp: D3D11_BLEND_OP_ADD,
      SrcBlendAlpha: D3D11_BLEND_ONE,
      DestBlendAlpha: D3D11_BLEND_INV_SRC_ALPHA,
      BlendOpAlpha: D3D11_BLEND_OP_ADD,
      RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
    };
    let mut layer_blend = None;
    unsafe {
      device.CreateBlendState(
        &D3D11_BLEND_DESC {
          RenderTarget: [layer_target; 8],
          ..Default::default()
        },
        Some(&mut layer_blend),
      )
    }
    .map_err(|error| error.to_string())?;
    let cursors = [
      (IDC_ARROW, "Arrow", "aero_arrow.cur"),
      (IDC_IBEAM, "IBeam", "beam_r.cur"),
      (IDC_IBEAM, "IBeam", "beam_r.cur"),
      (IDC_SIZEWE, "SizeWE", "aero_ew.cur"),
      (IDC_SIZENS, "SizeNS", "aero_ns.cur"),
      (IDC_HAND, "Hand", "aero_link.cur"),
      (IDC_CROSS, "Crosshair", "cross_r.cur"),
      (IDC_NO, "No", "aero_unavail.cur"),
    ];
    let cursor_pixels = cursors
      .into_iter()
      .map(|(cursor, scheme_value, fallback_file)| {
        native_cursor_pixels(cursor, scheme_value, fallback_file)
      })
      .collect::<Result<Vec<_>, _>>()?;
    let (cursor_width, cursor_height, _, _) = cursor_pixels[0];
    if cursor_pixels
      .iter()
      .any(|(width, height, _, _)| (*width, *height) != (cursor_width, cursor_height))
    {
      return Err("Windows standard cursors do not share one bitmap size".to_owned());
    }
    let cursor_description = D3D11_TEXTURE2D_DESC {
      Width: cursor_width,
      Height: cursor_height,
      MipLevels: 1,
      ArraySize: cursor_pixels.len() as u32,
      Format: DXGI_FORMAT_B8G8R8A8_UNORM,
      SampleDesc: DXGI_SAMPLE_DESC {
        Count: 1,
        Quality: 0,
      },
      Usage: D3D11_USAGE_DEFAULT,
      BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
      ..Default::default()
    };
    let initial_data = cursor_pixels
      .iter()
      .map(|(_, _, _, pixels)| D3D11_SUBRESOURCE_DATA {
        pSysMem: pixels.as_ptr().cast(),
        SysMemPitch: cursor_width * 4,
        ..Default::default()
      })
      .collect::<Vec<_>>();
    let mut cursor_texture = None;
    unsafe {
      device.CreateTexture2D(
        &cursor_description,
        Some(initial_data.as_ptr()),
        Some(&mut cursor_texture),
      )
    }
    .map_err(|error| error.to_string())?;
    let cursor_texture =
      cursor_texture.ok_or_else(|| "D3D11 created no native cursor texture atlas".to_owned())?;
    let cursor_resource: ID3D11Resource =
      cursor_texture.cast().map_err(|error| error.to_string())?;
    let mut cursor_view = None;
    unsafe { device.CreateShaderResourceView(&cursor_resource, None, Some(&mut cursor_view)) }
      .map_err(|error| error.to_string())?;
    let cursor_hotspots = std::array::from_fn(|index| {
      let hotspot = cursor_pixels[index].2;
      [hotspot[0], hotspot[1], 0.0, 0.0]
    });
    Ok(Self {
      constants: constants.ok_or_else(|| "D3D11 created no preview constant buffer".to_owned())?,
      cursor_hotspots,
      cursor_view: cursor_view
        .ok_or_else(|| "D3D11 created no native cursor atlas view".to_owned())?,
      layer_blend: layer_blend
        .ok_or_else(|| "D3D11 created no screenshot layer blend state".to_owned())?,
      pixel_shader: pixel_shader
        .ok_or_else(|| "D3D11 created no preview pixel shader".to_owned())?,
      sampler: sampler.ok_or_else(|| "D3D11 created no preview sampler".to_owned())?,
      vertex_shader: vertex_shader
        .ok_or_else(|| "D3D11 created no preview vertex shader".to_owned())?,
    })
  }

  pub(super) fn source(
    &self,
    device: &ID3D11Device,
    size: (u32, u32),
  ) -> Result<SourceTexture, String> {
    let description = D3D11_TEXTURE2D_DESC {
      Width: size.0,
      Height: size.1,
      MipLevels: 1,
      ArraySize: 1,
      Format: DXGI_FORMAT_B8G8R8A8_UNORM,
      SampleDesc: DXGI_SAMPLE_DESC {
        Count: 1,
        Quality: 0,
      },
      Usage: D3D11_USAGE_DEFAULT,
      BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
      ..Default::default()
    };
    let mut texture = None;
    unsafe { device.CreateTexture2D(&description, None, Some(&mut texture)) }
      .map_err(|error| error.to_string())?;
    let texture = texture.ok_or_else(|| "D3D11 created no preview source texture".to_owned())?;
    let resource: ID3D11Resource = texture.cast().map_err(|error| error.to_string())?;
    let mut view = None;
    unsafe { device.CreateShaderResourceView(&resource, None, Some(&mut view)) }
      .map_err(|error| error.to_string())?;
    Ok(SourceTexture {
      size,
      texture,
      view: view.ok_or_else(|| "D3D11 created no preview source view".to_owned())?,
    })
  }

  pub(super) fn screenshot_source(
    &self,
    device: &ID3D11Device,
    source: &crate::screenshots::CapturedImage,
  ) -> Result<SourceTexture, String> {
    if source.rgba.len() != source.width as usize * source.height as usize * 4 {
      return Err("The screenshot preview source pixels are not valid".to_owned());
    }
    let description = D3D11_TEXTURE2D_DESC {
      Width: source.width,
      Height: source.height,
      MipLevels: 1,
      ArraySize: 1,
      Format: DXGI_FORMAT_R8G8B8A8_UNORM,
      SampleDesc: DXGI_SAMPLE_DESC {
        Count: 1,
        Quality: 0,
      },
      Usage: D3D11_USAGE_DEFAULT,
      BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
      ..Default::default()
    };
    let initial_data = D3D11_SUBRESOURCE_DATA {
      pSysMem: source.rgba.as_ptr().cast(),
      SysMemPitch: source.width * 4,
      ..Default::default()
    };
    let mut texture = None;
    unsafe { device.CreateTexture2D(&description, Some(&initial_data), Some(&mut texture)) }
      .map_err(|error| error.to_string())?;
    let texture =
      texture.ok_or_else(|| "D3D11 created no screenshot preview texture".to_owned())?;
    let resource: ID3D11Resource = texture.cast().map_err(|error| error.to_string())?;
    let mut view = None;
    unsafe { device.CreateShaderResourceView(&resource, None, Some(&mut view)) }
      .map_err(|error| error.to_string())?;
    Ok(SourceTexture {
      size: (source.width, source.height),
      texture,
      view: view.ok_or_else(|| "D3D11 created no screenshot preview view".to_owned())?,
    })
  }

  #[allow(clippy::too_many_arguments)]
  pub(super) fn draw_with_camera(
    &self,
    context: &ID3D11DeviceContext,
    target: &ID3D11Texture2D,
    source: &SourceTexture,
    settings: &ScreenshotOutputSettings,
    composition: super::ComposedFrame,
    camera: Option<(&SourceTexture, BakeGeometry, bool, bool)>,
  ) -> Result<(), String> {
    let placement = output_placement(source.size.0, source.size.1, settings)?;
    let mut mesh_points = [[0.0; 4]; 8];
    let mut mesh_colors = [[0.0; 4]; 4];
    let mesh = settings.background_type == "mesh";
    if !mesh && settings.background_type != "solid" {
      return Err("The screenshot background type is not valid".to_owned());
    }
    if !settings.radius_percent.is_finite()
      || !(0.0..=50.0).contains(&settings.radius_percent)
      || !settings.background_radius_percent.is_finite()
      || !(0.0..=50.0).contains(&settings.background_radius_percent)
    {
      return Err("The screenshot canvas settings are not valid".to_owned());
    }
    if mesh {
      validate_mesh(
        &settings.mesh_colors,
        &settings.mesh_points,
        settings.mesh_warp_percent,
      )?;
      for (index, point) in settings.mesh_points.iter().enumerate() {
        let radians = point.rotation.to_radians();
        mesh_points[index * 2] = [
          (point.x / 100.0) as f32,
          (point.y / 100.0) as f32,
          (point.radius_x / 100.0) as f32,
          (point.radius_y / 100.0) as f32,
        ];
        mesh_points[index * 2 + 1] = [radians.cos() as f32, radians.sin() as f32, 0.0, 0.0];
        mesh_colors[index] = color(&settings.mesh_colors[index])?;
      }
    }
    let shortest_output = settings.width.min(settings.height) as f32;
    let visible_left = (placement.crop_x as f32).max(placement.image_x as f32);
    let visible_top = (placement.crop_y as f32).max(placement.image_y as f32);
    let visible_right = (placement.crop_x as f32 + placement.crop_width as f32)
      .min(placement.image_x as f32 + placement.image_width as f32);
    let visible_bottom = (placement.crop_y as f32 + placement.crop_height as f32)
      .min(placement.image_y as f32 + placement.image_height as f32);
    let visible_width = (visible_right - visible_left).max(0.0);
    let visible_height = (visible_bottom - visible_top).max(0.0);
    let shadow_margin = visible_left
      .min(visible_top)
      .min(settings.width as f32 - visible_right)
      .min(settings.height as f32 - visible_bottom)
      .max(0.0);
    let shadow_sigma = (visible_width.min(visible_height) * 0.055)
      .clamp(10.0, 110.0)
      .min(shadow_margin * 0.45);
    let shadow =
      settings.drop_shadow && visible_width > 0.0 && visible_height > 0.0 && shadow_sigma > 1.0;
    let shortest_crop = placement.crop_width.min(placement.crop_height) as f32;
    let values = Constants {
      output_source: [
        settings.width as f32,
        settings.height as f32,
        source.size.0 as f32,
        source.size.1 as f32,
      ],
      image_rect: [
        placement.image_x as f32,
        placement.image_y as f32,
        placement.image_width as f32,
        placement.image_height as f32,
      ],
      crop_rect: [
        placement.crop_x as f32,
        placement.crop_y as f32,
        placement.crop_width as f32,
        placement.crop_height as f32,
      ],
      solid_color: color(&settings.background_color)?,
      base_color: if mesh {
        color(
          settings
            .mesh_colors
            .last()
            .expect("a validated mesh has a base colour"),
        )?
      } else {
        [0.0; 4]
      },
      mesh_points,
      mesh_colors,
      effects: [
        shortest_crop * (settings.radius_percent as f32 / 100.0),
        shortest_output * (settings.background_radius_percent as f32 / 100.0),
        settings.mesh_warp_percent as f32,
        shadow_sigma,
      ],
      motion: [composition.seconds as f32, 0.0, 0.0, 0.0],
      cursor_geometry: composition.cursor.map_or([0.0; 4], |cursor| {
        [cursor.x, cursor.y, cursor.width, cursor.height]
      }),
      cursor_effects: composition.cursor.map_or([0.0; 4], |cursor| {
        [0.0, 0.0, cursor.rotation_radians, cursor.scale]
      }),
      cursor_blur: composition.cursor.map_or([0.0; 4], |cursor| {
        [cursor.blur_delta_x, cursor.blur_delta_y, 0.0, 0.0]
      }),
      camera_frame: camera.map_or([0.0; 4], |(_, geometry, _, _)| {
        [
          geometry.frame_x as f32,
          geometry.frame_y as f32,
          geometry.frame_width as f32,
          geometry.frame_height as f32,
        ]
      }),
      camera_crop: camera.map_or([0.0; 4], |(_, geometry, _, _)| {
        [
          geometry.crop_x as f32,
          geometry.crop_y as f32,
          geometry.crop_width as f32,
          geometry.crop_height as f32,
        ]
      }),
      camera_effects: camera.map_or([0.0; 4], |(_, geometry, drop_shadow, camera_on_top)| {
        let shortest = geometry.frame_width.min(geometry.frame_height) as f32;
        let sigma = if drop_shadow {
          (shortest * 0.055).clamp(3.0, 110.0)
        } else {
          0.0
        };
        [
          geometry.radius as f32,
          1.0,
          sigma,
          if camera_on_top { 1.0 } else { 0.0 },
        ]
      }),
      native_cursor_hotspots: self.cursor_hotspots,
      options: [
        settings.mesh_seed,
        u32::from(mesh),
        settings.mesh_points.len() as u32,
        u32::from(shadow),
      ],
      cursor_options: [
        composition.cursor.map_or(0, |cursor| cursor.style),
        u32::from(composition.cursor.is_some()),
        u32::from(
          composition
            .cursor
            .is_some_and(|cursor| cursor.clip_at_video_edge),
        ),
        u32::from(composition.foreground_only),
      ],
    };
    let target_resource: ID3D11Resource = target.cast().map_err(|error| error.to_string())?;
    let mut render_target: Option<ID3D11RenderTargetView> = None;
    unsafe {
      self
        .constants
        .cast::<ID3D11Resource>()
        .map_err(|error| error.to_string())
        .map(|resource| {
          context.UpdateSubresource(
            &resource,
            0,
            None,
            (&raw const values).cast::<c_void>(),
            0,
            0,
          );
        })?;
    }
    let device = unsafe { target.GetDevice() }.map_err(|error| error.to_string())?;
    unsafe { device.CreateRenderTargetView(&target_resource, None, Some(&mut render_target)) }
      .map_err(|error| error.to_string())?;
    let render_target =
      render_target.ok_or_else(|| "D3D11 created no preview render target".to_owned())?;
    let viewport = D3D11_VIEWPORT {
      Width: settings.width as f32,
      Height: settings.height as f32,
      MaxDepth: 1.0,
      ..Default::default()
    };
    unsafe {
      context.OMSetRenderTargets(Some(&[Some(render_target)]), None);
      context.OMSetBlendState(
        composition.foreground_only.then_some(&self.layer_blend),
        None,
        u32::MAX,
      );
      context.RSSetViewports(Some(&[viewport]));
      context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
      context.VSSetShader(&self.vertex_shader, None);
      context.PSSetShader(&self.pixel_shader, None);
      context.PSSetConstantBuffers(0, Some(&[Some(self.constants.clone())]));
      context.PSSetShaderResources(
        0,
        Some(&[
          Some(source.view.clone()),
          Some(self.cursor_view.clone()),
          camera.map(|(camera, _, _, _)| camera.view.clone()),
        ]),
      );
      context.PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));
      context.Draw(3, 0);
      context.PSSetShaderResources(0, Some(&[None, None, None]));
      context.OMSetBlendState(None::<&ID3D11BlendState>, None, u32::MAX);
      context.OMSetRenderTargets(None, None);
    }
    Ok(())
  }

  pub(super) fn copy_source(
    context: &ID3D11DeviceContext,
    destination: &SourceTexture,
    source: &ID3D11Texture2D,
    subresource: u32,
  ) -> Result<(), String> {
    let destination: ID3D11Resource = destination
      .texture
      .cast()
      .map_err(|error| error.to_string())?;
    let source: ID3D11Resource = source.cast().map_err(|error| error.to_string())?;
    unsafe { context.CopySubresourceRegion(&destination, 0, 0, 0, 0, &source, subresource, None) };
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn preview_shader_is_embedded_as_compiled_bytecode() {
    assert_eq!(&VERTEX_SHADER[..4], b"DXBC");
    assert_eq!(&PIXEL_SHADER[..4], b"DXBC");
  }
}
