// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Transparent D3D11 selection overlay composed above the preview panes.

use std::ffi::c_void;

use windows::{
  core::{Interface, PCWSTR},
  Win32::{
    Foundation::COLORREF,
    Graphics::{
      Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
      Direct3D11::{
        ID3D11Buffer, ID3D11Device, ID3D11DeviceContext, ID3D11PixelShader, ID3D11RenderTargetView,
        ID3D11Resource, ID3D11SamplerState, ID3D11ShaderResourceView, ID3D11Texture2D,
        ID3D11VertexShader, D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_SHADER_RESOURCE,
        D3D11_BUFFER_DESC, D3D11_FILTER_MIN_MAG_MIP_LINEAR, D3D11_SAMPLER_DESC,
        D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC, D3D11_TEXTURE_ADDRESS_CLAMP,
        D3D11_USAGE_DEFAULT, D3D11_USAGE_IMMUTABLE, D3D11_VIEWPORT,
      },
      DirectComposition::{IDCompositionDevice, IDCompositionVisual},
      Dxgi::{
        Common::{DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
        IDXGIFactory2, IDXGISwapChain3, DXGI_PRESENT, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1,
        DXGI_SWAP_CHAIN_FLAG, DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT,
      },
      Gdi::{
        CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject,
        GetTextExtentPoint32W, SelectObject, SetBkMode, SetTextColor, TextOutW,
        ANTIALIASED_QUALITY, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CLIP_DEFAULT_PRECIS,
        DEFAULT_CHARSET, DIB_RGB_COLORS, FF_MODERN, FIXED_PITCH, FW_MEDIUM, OUT_DEFAULT_PRECIS,
        TRANSPARENT,
      },
    },
  },
};

// Extracted by build.rs at compile time; never read from Rust.
#[allow(dead_code)]
const SHADER: &str = r#"
cbuffer Selection : register(b0) {
  float4 frame;       // viewport-local x/y/width/height in physical pixels
  float4 viewport;    // physical width/height, theme (0 dark, 1 light), visible
  float4 radius_control; // center x/y, visible, reserved
  float4 guides; // x, y, x-is-object, y-is-object (negative x/y means hidden)
  float4 crop_image; // image x/y/width/height; negative width disables crop shade
  float4 magnifier_box; // x/y/width/height; zero width disables the cutout
  float4 label; // size readout x/y/width/height in physical pixels; zero width hides it
  float4 label_params; // halo radius in pixels, display scale (pixels per point), reserved
};

// Grayscale glyph coverage of the "W x H" readout, one texel per physical
// pixel of `label`, rasterised by GDI on the CPU (see `LabelTexture`).
Texture2D label_coverage : register(t0);
SamplerState label_sampler : register(s0);

struct VertexOut { float4 position : SV_Position; };
VertexOut vs_main(uint id : SV_VertexID) {
  VertexOut output;
  float2 p = float2((id << 1) & 2, id & 2);
  output.position = float4(p * float2(2, -2) + float2(-1, 1), 0, 1);
  return output;
}

float circle_coverage(float2 pixel, float2 center, float radius) {
  // Like the Metal handle quads: fully opaque up to the radius, with the AA
  // ramp spent outside the edge, so the fill keeps its colour to the rim and
  // the ring around it stays one pixel wide.
  float distance = length(pixel - center) - radius;
  return 1.0 - smoothstep(0.0, 1.0, distance);
}

float line_coverage(float value, float edge, float half_width) {
  // Hard-edged like the Metal quads: full coverage inside the half width, a
  // one-pixel falloff outside, so a 1px core lands on exactly one pixel row.
  return 1.0 - smoothstep(half_width - 0.5, half_width + 0.5, abs(value - edge));
}

float rounded_distance(float2 pixel, float4 rect, float radius) {
  float2 half_size = rect.zw * 0.5;
  float2 local = abs(pixel - (rect.xy + half_size)) - (half_size - radius);
  return length(max(local, 0.0)) + min(max(local.x, local.y), 0.0) - radius;
}

float label_sample(float2 uv) {
  return label_coverage.SampleLevel(label_sampler, uv, 0).r;
}

float4 ps_main(VertexOut input) : SV_Target {
  if (viewport.w < 0.5) return 0;
  float2 p = input.position.xy;
  if (magnifier_box.z > 0.0 &&
      rounded_distance(p, magnifier_box, max(magnifier_box.z / 24.0, 1.0)) <= 0.0)
    return 0;
  // Frame edges arrive on integer pixel boundaries while SV_Position samples
  // pixel centres; snap the lines onto the inner pixel row so the core is a
  // solid line rather than two half-covered grey ones.
  float left = frame.x + 0.5, top = frame.y + 0.5;
  float right = frame.x + frame.z - 0.5, bottom = frame.y + frame.w - 0.5;
  // Metal sizes the OSC in points: a 1pt core inside a 3pt halo, 4pt handles
  // with a one-device-pixel ring. Scale converts those to physical pixels.
  float scale = max(label_params.y, 0.1);
  float core_half = 0.5 * scale, halo_half = 1.5 * scale;
  float within_x = step(left - halo_half - 1.5, p.x) * step(p.x, right + halo_half + 1.5);
  float within_y = step(top - halo_half - 1.5, p.y) * step(p.y, bottom + halo_half + 1.5);
  float border = max(max(line_coverage(p.x, left, halo_half), line_coverage(p.x, right, halo_half)) * within_y,
                     max(line_coverage(p.y, top, halo_half), line_coverage(p.y, bottom, halo_half)) * within_x);
  float core = max(max(line_coverage(p.x, left, core_half), line_coverage(p.x, right, core_half)) * within_y,
                   max(line_coverage(p.y, top, core_half), line_coverage(p.y, bottom, core_half)) * within_x);
  if (crop_image.z >= 0.0) {
    float vertical_distance = min(abs(p.x - left), abs(p.x - right));
    float horizontal_distance = min(abs(p.y - top), abs(p.y - bottom));
    float coordinate = vertical_distance < horizontal_distance ? p.y - top : p.x - left;
    float wave = abs(frac(coordinate / 10.0) - 0.5);
    float aa = max(fwidth(wave), 0.001);
    float dash = 1.0 - smoothstep(0.30, 0.30 + aa, wave);
    border *= dash;
    core *= dash;
  }
  float2 handles[8] = {
    float2(left, top), float2((left + right) * 0.5, top), float2(right, top),
    float2(right, (top + bottom) * 0.5), float2(right, bottom),
    float2((left + right) * 0.5, bottom), float2(left, bottom),
    float2(left, (top + bottom) * 0.5)
  };
  float handle_radius = 4.0 * scale;
  float handle_ring = handle_radius + 1.0;
  float handle_outline = 0.0, handle_fill = 0.0;
  [unroll] for (uint index = 0; index < 8; ++index) {
    handle_outline = max(handle_outline, circle_coverage(p, handles[index], handle_ring));
    handle_fill = max(handle_fill, circle_coverage(p, handles[index], handle_radius));
  }
  if (radius_control.z > 0.5) {
    handle_outline = max(handle_outline, circle_coverage(p, radius_control.xy, handle_ring));
    handle_fill = max(handle_fill, circle_coverage(p, radius_control.xy, handle_radius));
  }
  float dark_theme = 1.0 - viewport.z;
  float3 primary = lerp(float3(0.09, 0.09, 0.10), 1.0, dark_theme);
  float3 contrast = lerp(1.0, float3(0.09, 0.09, 0.10), dark_theme);
  float contrast_alpha = saturate(max(border, handle_outline));
  float primary_alpha = saturate(max(core, handle_fill));
  // The crop shade is the bottom layer, as on macOS, so the border, handles
  // and guides composite over it at full strength instead of being dimmed.
  float crop_shade = 0.0;
  if (crop_image.z >= 0.0) {
    float image_inside = step(crop_image.x, p.x) * step(p.x, crop_image.x + crop_image.z) *
                         step(crop_image.y, p.y) * step(p.y, crop_image.y + crop_image.w);
    float crop_inside = step(frame.x, p.x) * step(p.x, frame.x + frame.z) *
                        step(frame.y, p.y) * step(p.y, frame.y + frame.w);
    crop_shade = image_inside * (1.0 - crop_inside);
  }
  float shade_alpha = crop_shade * 0.4;
  float3 color = float3(0.0, 0.0, 0.0);
  float alpha = shade_alpha;
  color = lerp(color, contrast, contrast_alpha);
  alpha = max(alpha, contrast_alpha);
  color = lerp(color, primary, primary_alpha);
  alpha = max(alpha, primary_alpha);
  float guide_x = guides.x >= 0.0 ? line_coverage(p.x, guides.x, 0.5) : 0.0;
  float guide_y = guides.y >= 0.0 ? line_coverage(p.y, guides.y, 0.5) : 0.0;
  float guide_alpha = max(guide_x, guide_y);
  float3 canvas_guide = dark_theme > 0.5 ? float3(0.98, 0.75, 0.12) : float3(0.78, 0.46, 0.02);
  float3 object_guide = dark_theme > 0.5 ? float3(0.18, 0.70, 0.95) : float3(0.00, 0.42, 0.70);
  float3 guide_color = guide_x > 0.0 ? (guides.z > 0.5 ? object_guide : canvas_guide)
                                    : (guides.w > 0.5 ? object_guide : canvas_guide);
  color = lerp(color, guide_color, guide_alpha);
  alpha = max(alpha, guide_alpha);
  // The size readout sits on top of everything. It is the Metal backend's
  // stroked-then-filled monospaced label: the fill is the glyph coverage, the
  // halo a dilation of that coverage by the stroke radius, so the text stays
  // legible over any pane content without a backing plate.
  if (label.z > 0.0 && p.x >= label.x && p.x <= label.x + label.z &&
      p.y >= label.y && p.y <= label.y + label.w) {
    float2 uv = (p - label.xy) / label.zw;
    float2 texel = 1.0 / label.zw;
    float fill_coverage = label_sample(uv);
    // A stroke tapers: the halo is the mean coverage of a ring of taps at the
    // stroke radius, stretched so a fully covered neighbourhood saturates and
    // a lone edge fades, rather than a hard `max` dilation.
    float halo_radius = label_params.x;
    float ring = 0.0;
    [unroll] for (uint tap = 0; tap < 8; ++tap) {
      float angle = tap * 0.78539816;
      float2 direction = float2(cos(angle), sin(angle));
      ring += label_sample(uv + direction * halo_radius * texel);
    }
    float halo_coverage = max(fill_coverage, saturate(ring * 0.5));
    float3 label_fill = lerp(float3(0.12, 0.12, 0.12), 1.0, dark_theme);
    float3 label_halo = lerp(1.0, 0.0, dark_theme);
    float halo_alpha = saturate(halo_coverage) * lerp(1.0, 0.8, dark_theme);
    color = lerp(color, label_halo, halo_alpha);
    alpha = max(alpha, halo_alpha);
    color = lerp(color, label_fill, saturate(fill_coverage));
    alpha = max(alpha, saturate(fill_coverage));
  }
  return float4(color * alpha, alpha);
}
"#;

const VERTEX_SHADER: &[u8] =
  include_bytes!(concat!(env!("OUT_DIR"), "/recording_selection_vs.cso"));
const PIXEL_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/recording_selection_ps.cso"));

/// Point size of the size readout's monospaced font, as on macOS.
const LABEL_FONT_SIZE: f64 = 11.0;
/// Width of the halo stroke in points; it is centred on the glyph outline, so
/// half of it spills outside the glyph and is what the shader dilates by.
const LABEL_STROKE: f64 = 2.0;
/// Transparent padding around the glyph box in points, room for the halo.
const LABEL_PADDING: f64 = 2.0;

#[repr(C)]
#[derive(Clone, Copy)]
struct Constants {
  frame: [f32; 4],
  viewport: [f32; 4],
  radius_control: [f32; 4],
  guides: [f32; 4],
  crop_image: [f32; 4],
  magnifier_box: [f32; 4],
  label: [f32; 4],
  label_params: [f32; 4],
}

/// The rasterised "W × H" readout and the inputs it was built from. Rebuilding
/// the bitmap on every gesture sample would burn a GDI text layout per pointer
/// event, so the texture is reused until the text or the scale changes. The
/// theme is applied in the shader, so it needs no rebuild.
struct LabelTexture {
  /// Kept alive for the view; never read back.
  _texture: ID3D11Texture2D,
  scale_key: u32,
  size: (u32, u32),
  text: String,
  view: ID3D11ShaderResourceView,
}

fn label_scale_key(scale: f64) -> u32 {
  (scale * 1000.0).round().max(0.0) as u32
}

/// Rasterises `text` with GDI as white-on-black grayscale coverage: a
/// monospaced medium-weight face at `LABEL_FONT_SIZE` points scaled by the
/// display scale, padded by `LABEL_PADDING` points on every side. The red
/// channel of the returned BGRA bitmap is the glyph coverage.
fn rasterize_label(text: &str, scale: f64) -> Result<(Vec<u8>, (u32, u32)), String> {
  let wide: Vec<u16> = text.encode_utf16().collect();
  let dc = unsafe { CreateCompatibleDC(None) };
  if dc.is_invalid() {
    return Err("Windows could not create a label drawing context".to_owned());
  }
  let face: Vec<u16> = "Consolas\0".encode_utf16().collect();
  let font = unsafe {
    CreateFontW(
      -((LABEL_FONT_SIZE * scale).round() as i32).max(1),
      0,
      0,
      0,
      FW_MEDIUM.0 as i32,
      0,
      0,
      0,
      DEFAULT_CHARSET,
      OUT_DEFAULT_PRECIS,
      CLIP_DEFAULT_PRECIS,
      ANTIALIASED_QUALITY,
      (FIXED_PITCH.0 | FF_MODERN.0) as u32,
      PCWSTR(face.as_ptr()),
    )
  };
  if font.is_invalid() {
    let _ = unsafe { DeleteDC(dc) };
    return Err("Windows could not create the label font".to_owned());
  }
  let old_font = unsafe { SelectObject(dc, font.into()) };
  let mut extent = Default::default();
  let measured = unsafe { GetTextExtentPoint32W(dc, &wide, &mut extent) };
  if !measured.as_bool() || extent.cx <= 0 || extent.cy <= 0 {
    unsafe {
      SelectObject(dc, old_font);
      let _ = DeleteObject(font.into());
      let _ = DeleteDC(dc);
    }
    return Err("Windows could not measure the label text".to_owned());
  }
  let padding = (LABEL_PADDING * scale).ceil() as i32;
  let width = (extent.cx + padding * 2).max(1);
  let height = (extent.cy + padding * 2).max(1);
  let info = BITMAPINFO {
    bmiHeader: BITMAPINFOHEADER {
      biSize: size_of::<BITMAPINFOHEADER>() as u32,
      biWidth: width,
      biHeight: -height,
      biPlanes: 1,
      biBitCount: 32,
      biCompression: BI_RGB.0,
      ..Default::default()
    },
    ..Default::default()
  };
  let mut bits = std::ptr::null_mut();
  let bitmap = match unsafe {
    CreateDIBSection(
      Some(dc),
      &raw const info,
      DIB_RGB_COLORS,
      &mut bits,
      None,
      0,
    )
  } {
    Ok(bitmap) => bitmap,
    Err(error) => {
      unsafe {
        SelectObject(dc, old_font);
        let _ = DeleteObject(font.into());
        let _ = DeleteDC(dc);
      }
      return Err(error.to_string());
    }
  };
  let old_bitmap = unsafe { SelectObject(dc, bitmap.into()) };
  let length = (width * height * 4) as usize;
  let pixels = unsafe { std::slice::from_raw_parts_mut(bits.cast::<u8>(), length) };
  // Black, opaque background: GDI antialiasing blends the white glyphs into
  // whatever is underneath, so the red channel ends up as plain coverage.
  for pixel in pixels.chunks_exact_mut(4) {
    pixel.fill(0);
    pixel[3] = 255;
  }
  let drawn = unsafe {
    SetBkMode(dc, TRANSPARENT);
    SetTextColor(dc, COLORREF(0x00FF_FFFF));
    TextOutW(dc, padding, padding, &wide)
  };
  let result = if drawn.as_bool() {
    Ok((pixels.to_vec(), (width as u32, height as u32)))
  } else {
    Err("Windows could not draw the label text".to_owned())
  };
  unsafe {
    SelectObject(dc, old_bitmap);
    SelectObject(dc, old_font);
    let _ = DeleteObject(bitmap.into());
    let _ = DeleteObject(font.into());
    let _ = DeleteDC(dc);
  }
  result
}

/// Uploads `pixels` (BGRA, `size`) as an immutable shader-readable texture.
fn upload_label_texture(
  device: &ID3D11Device,
  pixels: &[u8],
  size: (u32, u32),
  text: &str,
  scale_key: u32,
) -> Result<LabelTexture, String> {
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
    Usage: D3D11_USAGE_IMMUTABLE,
    BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
    ..Default::default()
  };
  let initial = D3D11_SUBRESOURCE_DATA {
    pSysMem: pixels.as_ptr().cast::<c_void>(),
    SysMemPitch: size.0 * 4,
    SysMemSlicePitch: 0,
  };
  let mut texture = None;
  unsafe { device.CreateTexture2D(&description, Some(&initial), Some(&mut texture)) }
    .map_err(|error| error.to_string())?;
  let texture = texture.ok_or_else(|| "D3D11 created no selection label texture".to_owned())?;
  let resource: ID3D11Resource = texture.cast().map_err(|error| error.to_string())?;
  let mut view = None;
  unsafe { device.CreateShaderResourceView(&resource, None, Some(&mut view)) }
    .map_err(|error| error.to_string())?;
  Ok(LabelTexture {
    _texture: texture,
    scale_key,
    size,
    text: text.to_owned(),
    view: view.ok_or_else(|| "D3D11 created no selection label view".to_owned())?,
  })
}

fn build_label_texture(
  device: &ID3D11Device,
  text: &str,
  scale: f64,
) -> Result<LabelTexture, String> {
  let (pixels, size) = rasterize_label(text, scale)?;
  upload_label_texture(device, &pixels, size, text, label_scale_key(scale))
}

pub(super) struct SelectionOverlay {
  buffer_size: (u32, u32),
  constants: ID3D11Buffer,
  label: Option<LabelTexture>,
  /// Bound whenever there is no label, so the pixel shader's texture slot is
  /// always filled with a real (transparent 1x1) view.
  label_placeholder: LabelTexture,
  label_sampler: ID3D11SamplerState,
  pixel_shader: ID3D11PixelShader,
  swap_chain: IDXGISwapChain3,
  vertex_shader: ID3D11VertexShader,
  /// Held only to keep the composition visual alive: the swap chain is
  /// attached once and no property is mutated after construction.
  _visual: IDCompositionVisual,
}

impl SelectionOverlay {
  pub(super) fn new(
    device: &ID3D11Device,
    factory: &IDXGIFactory2,
    composition: &IDCompositionDevice,
    root: &IDCompositionVisual,
  ) -> Result<Self, String> {
    let description = DXGI_SWAP_CHAIN_DESC1 {
      Width: 2,
      Height: 2,
      Format: DXGI_FORMAT_B8G8R8A8_UNORM,
      SampleDesc: DXGI_SAMPLE_DESC {
        Count: 1,
        Quality: 0,
      },
      BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
      BufferCount: 2,
      Scaling: DXGI_SCALING_STRETCH,
      SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
      AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
      ..Default::default()
    };
    let swap_chain = unsafe { factory.CreateSwapChainForComposition(device, &description, None) }
      .and_then(|chain| chain.cast::<IDXGISwapChain3>())
      .map_err(|error| format!("The Windows selection swap chain could not be created: {error}"))?;
    let visual = unsafe { composition.CreateVisual() }.map_err(|error| error.to_string())?;
    unsafe {
      visual
        .SetContent(&swap_chain)
        .map_err(|error| error.to_string())?;
      root
        .AddVisual(&visual, true, None::<&IDCompositionVisual>)
        .map_err(|error| error.to_string())?;
    }
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
    let mut constants = None;
    unsafe {
      device
        .CreateBuffer(
          &D3D11_BUFFER_DESC {
            ByteWidth: size_of::<Constants>() as u32,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            ..Default::default()
          },
          None,
          Some(&mut constants),
        )
        .map_err(|error| error.to_string())?;
    }
    let sampler_description = D3D11_SAMPLER_DESC {
      Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
      AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
      AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
      AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
      MaxLOD: f32::MAX,
      ..Default::default()
    };
    let mut label_sampler = None;
    unsafe { device.CreateSamplerState(&sampler_description, Some(&mut label_sampler)) }
      .map_err(|error| error.to_string())?;
    // A 1x1 transparent texture stands in whenever no size readout exists.
    let label_placeholder = upload_label_texture(device, &[0u8; 4], (1, 1), "", 0)?;
    Ok(Self {
      buffer_size: (2, 2),
      constants: constants.ok_or_else(|| "D3D11 created no selection constants".to_owned())?,
      label: None,
      label_placeholder,
      label_sampler: label_sampler
        .ok_or_else(|| "D3D11 created no selection label sampler".to_owned())?,
      pixel_shader: pixel_shader
        .ok_or_else(|| "D3D11 created no selection pixel shader".to_owned())?,
      swap_chain,
      vertex_shader: vertex_shader
        .ok_or_else(|| "D3D11 created no selection vertex shader".to_owned())?,
      _visual: visual,
    })
  }

  /// Returns the label texture for `text` at `scale`, building it only when
  /// either changed since the last draw. `None` when GDI could not rasterise
  /// it, in which case no label is drawn.
  fn label_texture(
    &mut self,
    device: &ID3D11Device,
    text: &str,
    scale: f64,
  ) -> Option<&LabelTexture> {
    let key = label_scale_key(scale);
    let stale = self
      .label
      .as_ref()
      .is_none_or(|label| label.scale_key != key || label.text != text);
    if stale {
      self.label = build_label_texture(device, text, scale)
        .inspect_err(|error| eprintln!("The selection size readout could not be drawn: {error}"))
        .ok();
    }
    self.label.as_ref()
  }

  #[allow(clippy::too_many_arguments)]
  pub(super) fn draw(
    &mut self,
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    viewport_size: (u32, u32),
    frame: Option<[f32; 4]>,
    radius_point: Option<[f32; 2]>,
    crop_image: Option<[f32; 4]>,
    guides: Option<(Option<f32>, Option<f32>, bool, bool)>,
    magnifier_box: Option<[f32; 4]>,
    label_text: Option<&str>,
    scale: f64,
    light: bool,
  ) -> Result<(), String> {
    let size = (viewport_size.0.max(2), viewport_size.1.max(2));
    if size != self.buffer_size {
      unsafe {
        self.swap_chain.ResizeBuffers(
          2,
          size.0,
          size.1,
          DXGI_FORMAT_B8G8R8A8_UNORM,
          DXGI_SWAP_CHAIN_FLAG(0),
        )
      }
      .map_err(|error| format!("The Windows selection overlay could not resize: {error}"))?;
      self.buffer_size = size;
    }
    let scale = scale.max(0.1);
    // The readout hangs 4pt below the box, trailing edge flush with the box's
    // right edge (Keyframeless's placement), flips above the box when it would
    // run off the bottom, and is clamped into the viewport. Everything here is
    // already in physical pixels, so snapping to the pixel grid is a floor:
    // the glyphs then land on the grid they were rasterised on and stay crisp.
    let label = match (frame, label_text) {
      (Some(frame), Some(text)) => self
        .label_texture(device, text, scale)
        .map(|label| (label.view.clone(), label.size))
        .map(|(view, (width, height))| {
          let width = width as f32;
          let height = height as f32;
          let gap = (4.0 * scale) as f32;
          let mut x = frame[0] + frame[2] - width;
          let mut y = frame[1] + frame[3] + gap;
          if y + height > size.1 as f32 {
            y = frame[1] + frame[3] - gap - height;
          }
          x = x.min(size.0 as f32 - width).max(0.0);
          y = y.max(0.0);
          (view, [x.floor(), y.floor(), width, height])
        }),
      _ => None,
    };
    let (label_view, label_rect) = match label {
      Some((view, rect)) => (view, rect),
      None => (self.label_placeholder.view.clone(), [0.0; 4]),
    };
    let values = Constants {
      frame: frame.unwrap_or_default(),
      viewport: [
        size.0 as f32,
        size.1 as f32,
        u32::from(light) as f32,
        f32::from(frame.is_some()),
      ],
      radius_control: radius_point.map_or([0.0; 4], |point| [point[0], point[1], 1.0, 0.0]),
      guides: guides.map_or([-1.0, -1.0, 0.0, 0.0], |(x, y, x_object, y_object)| {
        [
          x.unwrap_or(-1.0),
          y.unwrap_or(-1.0),
          if x_object { 1.0 } else { 0.0 },
          if y_object { 1.0 } else { 0.0 },
        ]
      }),
      crop_image: crop_image.unwrap_or([-1.0; 4]),
      magnifier_box: magnifier_box.unwrap_or_default(),
      label: label_rect,
      // The stroke is centred on the glyph outline, so half of it spills out;
      // GDI glyphs are lighter than CoreText's, and the ring-mean halo reaches
      // a touch past its radius, so it is trimmed to keep the same weight.
      label_params: [
        (LABEL_STROKE * 0.5 * 0.75 * scale) as f32,
        scale as f32,
        0.0,
        0.0,
      ],
    };
    let constants: ID3D11Resource = self.constants.cast().map_err(|error| error.to_string())?;
    unsafe {
      context.UpdateSubresource(
        &constants,
        0,
        None,
        (&raw const values).cast::<c_void>(),
        0,
        0,
      )
    };
    let index = unsafe { self.swap_chain.GetCurrentBackBufferIndex() };
    let texture = unsafe { self.swap_chain.GetBuffer::<ID3D11Texture2D>(index) }
      .map_err(|error| error.to_string())?;
    let resource: ID3D11Resource = texture.cast().map_err(|error| error.to_string())?;
    let mut target: Option<ID3D11RenderTargetView> = None;
    unsafe { device.CreateRenderTargetView(&resource, None, Some(&mut target)) }
      .map_err(|error| error.to_string())?;
    let target = target.ok_or_else(|| "D3D11 created no selection target".to_owned())?;
    unsafe {
      context.ClearRenderTargetView(&target, &[0.0; 4]);
      context.OMSetRenderTargets(Some(&[Some(target)]), None);
      context.RSSetViewports(Some(&[D3D11_VIEWPORT {
        Width: size.0 as f32,
        Height: size.1 as f32,
        MaxDepth: 1.0,
        ..Default::default()
      }]));
      context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
      context.VSSetShader(&self.vertex_shader, None);
      context.PSSetShader(&self.pixel_shader, None);
      context.PSSetConstantBuffers(0, Some(&[Some(self.constants.clone())]));
      context.PSSetShaderResources(0, Some(&[Some(label_view)]));
      context.PSSetSamplers(0, Some(&[Some(self.label_sampler.clone())]));
      context.Draw(3, 0);
      context.PSSetShaderResources(0, Some(&[None]));
      context.OMSetRenderTargets(None, None);
      self
        .swap_chain
        .Present(0, DXGI_PRESENT(0))
        .ok()
        .map_err(|error| error.to_string())?;
    }
    Ok(())
  }
}
