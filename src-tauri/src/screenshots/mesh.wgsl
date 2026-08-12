// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

struct MeshUniforms {
  dimensions: vec2<u32>,
  point_count: u32,
  seed: u32,
  warp_percent: f32,
  base_color: vec4<f32>,
  points: array<vec4<f32>, 8>,
  colors: array<vec4<f32>, 4>,
}

@group(0) @binding(0) var<uniform> mesh: MeshUniforms;
@group(0) @binding(1) var output: texture_storage_2d<rgba8unorm, write>;

fn hash(position: vec2<f32>, seed: u32) -> f32 {
  let value = sin(dot(position, vec2<f32>(127.1, 311.7)) + f32(seed) * 0.017) * 43758.5453;
  return fract(value) * 2.0 - 1.0;
}

fn noise(position: vec2<f32>, seed: u32) -> f32 {
  let cell = floor(position);
  let local = fract(position);
  let eased = local * local * (3.0 - 2.0 * local);
  let top = mix(hash(cell, seed), hash(cell + vec2<f32>(1.0, 0.0), seed), eased.x);
  let bottom = mix(hash(cell + vec2<f32>(0.0, 1.0), seed), hash(cell + vec2<f32>(1.0), seed), eased.x);
  return mix(top, bottom, eased.y);
}

fn fractal_noise(position: vec2<f32>, seed: u32) -> f32 {
  return noise(position, seed) * 0.58
    + noise(position * 2.07 + vec2<f32>(11.3, -4.9), seed ^ 0x68bc21ebu) * 0.28
    + noise(position * 4.19 + vec2<f32>(-8.7, 13.1), seed ^ 0x02e5be93u) * 0.14;
}

// Anti-banding only needs to move an 8-bit channel across its nearest
// quantisation boundary. Repeating a small noise tile keeps that variation
// visually irregular while allowing PNG's row filters and DEFLATE window to
// recognise it. Hashing the absolute coordinate made every background pixel
// unique and needlessly expensive to store losslessly.
fn antibanding_dither(id: vec2<u32>, seed: u32) -> f32 {
  let tile = vec2<f32>(id % vec2<u32>(64u));
  return hash(tile, seed) * 0.75 / 255.0;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
  if (id.x >= mesh.dimensions.x || id.y >= mesh.dimensions.y) {
    return;
  }
  let dimensions = vec2<f32>(mesh.dimensions);
  let shortest = min(dimensions.x, dimensions.y);
  let pixel = vec2<f32>(id.xy) + vec2<f32>(0.5);
  let aspect = vec2<f32>(dimensions.x / shortest, dimensions.y / shortest);
  let frequency = 3.5 / shortest;
  let warp_scale = shortest * mesh.warp_percent / 100.0;
  let warp = vec2<f32>(
    fractal_noise(pixel * frequency, mesh.seed),
    fractal_noise(pixel * frequency + vec2<f32>(19.7, -7.3), mesh.seed ^ 0xa511e9b3u),
  ) * warp_scale;
  var weighted_color = mesh.base_color.rgb * 0.18;
  var total_weight = 0.18;
  for (var index = 0u; index < mesh.point_count; index += 1u) {
    let first = mesh.points[index * 2u];
    let second = mesh.points[index * 2u + 1u];
    let delta = (pixel + warp) / shortest - first.xy * aspect;
    let rotated = vec2<f32>(
      delta.x * second.x + delta.y * second.y,
      -delta.x * second.y + delta.y * second.x,
    );
    let distance = length(rotated / max(first.zw, vec2<f32>(0.01)));
    let weight = 1.0 / (pow(max(distance, 0.025), 3.5) + 0.012);
    weighted_color += mesh.colors[index].rgb * weight;
    total_weight += weight;
  }
  let result = weighted_color / total_weight;
  let depth = fractal_noise(pixel * frequency * 0.7, mesh.seed ^ 0xd1b54a35u) * 13.0 / 255.0;
  let dither = antibanding_dither(id.xy, mesh.seed ^ 0x8da6b343u);
  textureStore(output, vec2<i32>(id.xy), vec4<f32>(clamp(result + depth + dither, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0));
}
