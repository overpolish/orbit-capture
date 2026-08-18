// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Transparent D3D11 selection overlay composed above the preview panes.

use std::ffi::c_void;

use windows::{
  core::Interface,
  Win32::Graphics::{
    Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
    Direct3D11::{
      ID3D11Buffer, ID3D11Device, ID3D11DeviceContext, ID3D11PixelShader, ID3D11RenderTargetView,
      ID3D11Resource, ID3D11Texture2D, ID3D11VertexShader, D3D11_BIND_CONSTANT_BUFFER,
      D3D11_BUFFER_DESC, D3D11_USAGE_DEFAULT, D3D11_VIEWPORT,
    },
    DirectComposition::{IDCompositionDevice, IDCompositionVisual},
    Dxgi::{
      Common::{DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
      IDXGIFactory2, IDXGISwapChain3, DXGI_PRESENT, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1,
      DXGI_SWAP_CHAIN_FLAG, DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT,
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
};

struct VertexOut { float4 position : SV_Position; };
VertexOut vs_main(uint id : SV_VertexID) {
  VertexOut output;
  float2 p = float2((id << 1) & 2, id & 2);
  output.position = float4(p * float2(2, -2) + float2(-1, 1), 0, 1);
  return output;
}

float circle_coverage(float2 pixel, float2 center, float radius) {
  float distance = length(pixel - center) - radius;
  return 1.0 - smoothstep(-0.75, 0.75, distance);
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
  float within_x = step(left - 3.0, p.x) * step(p.x, right + 3.0);
  float within_y = step(top - 3.0, p.y) * step(p.y, bottom + 3.0);
  float border = max(max(line_coverage(p.x, left, 1.5), line_coverage(p.x, right, 1.5)) * within_y,
                     max(line_coverage(p.y, top, 1.5), line_coverage(p.y, bottom, 1.5)) * within_x);
  float core = max(max(line_coverage(p.x, left, 0.5), line_coverage(p.x, right, 0.5)) * within_y,
                   max(line_coverage(p.y, top, 0.5), line_coverage(p.y, bottom, 0.5)) * within_x);
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
  float handle_outline = 0.0, handle_fill = 0.0;
  [unroll] for (uint index = 0; index < 8; ++index) {
    handle_outline = max(handle_outline, circle_coverage(p, handles[index], 5.5));
    handle_fill = max(handle_fill, circle_coverage(p, handles[index], 4.0));
  }
  if (radius_control.z > 0.5) {
    handle_outline = max(handle_outline, circle_coverage(p, radius_control.xy, 5.5));
    handle_fill = max(handle_fill, circle_coverage(p, radius_control.xy, 4.0));
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
  return float4(color * alpha, alpha);
}
"#;

const VERTEX_SHADER: &[u8] =
  include_bytes!(concat!(env!("OUT_DIR"), "/recording_selection_vs.cso"));
const PIXEL_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/recording_selection_ps.cso"));

#[repr(C)]
#[derive(Clone, Copy)]
struct Constants {
  frame: [f32; 4],
  viewport: [f32; 4],
  radius_control: [f32; 4],
  guides: [f32; 4],
  crop_image: [f32; 4],
  magnifier_box: [f32; 4],
}

pub(super) struct SelectionOverlay {
  buffer_size: (u32, u32),
  constants: ID3D11Buffer,
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
    Ok(Self {
      buffer_size: (2, 2),
      constants: constants.ok_or_else(|| "D3D11 created no selection constants".to_owned())?,
      pixel_shader: pixel_shader
        .ok_or_else(|| "D3D11 created no selection pixel shader".to_owned())?,
      swap_chain,
      vertex_shader: vertex_shader
        .ok_or_else(|| "D3D11 created no selection vertex shader".to_owned())?,
      _visual: visual,
    })
  }

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
      context.Draw(3, 0);
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
