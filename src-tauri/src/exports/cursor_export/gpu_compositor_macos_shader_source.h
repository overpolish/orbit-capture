// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

/// Native Metal shader extension point for future screenshot annotation tools.
__attribute__((visibility("hidden"))) NSString *const shader_source = @R"METAL(
#include <metal_stdlib>
using namespace metal;

struct GpuCursor {
  float blur_delta_x;
  float blur_delta_y;
  float height;
  float hotspot_x;
  float hotspot_y;
  float rotation_radians;
  float scale;
  float width;
  float x;
  float y;
  uint style;
  uint clip_at_video_edge;
  uint visible;
};

struct CursorArtwork {
  uint width;
  uint height;
  float design_width;
  float design_height;
  float origin_x;
  float origin_y;
  uint use_design;
  uint clip_local_box;
};

struct OverlayUniforms {
  int x;
  int y;
  uint cursor_width;
  uint cursor_height;
  uint output_width;
  uint output_height;
  int crop_x;
  int crop_y;
  uint crop_width;
  uint crop_height;
  uint crop_radius;
  uint clip_at_video_edge;
  struct GpuCursor cursor;
  struct CursorArtwork artwork;
};

struct CameraUniforms {
  uint crop_x;
  uint crop_y;
  uint crop_width;
  uint crop_height;
  int frame_x;
  int frame_y;
  uint frame_width;
  uint frame_height;
  uint radius;
  uint source_width;
  uint source_height;
  uint drop_shadow;
};

struct CanvasUniforms {
  packed_float4 background_color;
  uint background_radius;
  int crop_x;
  int crop_y;
  uint crop_width;
  uint crop_height;
  float image_x;
  float image_y;
  uint image_width;
  uint image_height;
  uint radius;
  uint drop_shadow;
  uint mesh_enabled;
  uint mesh_seed;
  float mesh_warp_percent;
  uint mesh_point_count;
  packed_float4 mesh_points[8];
  packed_float4 mesh_colors[5];
  uint clip_cursor_at_video_edge;
  uint transparent_background;
  uint foreground_only;
};

struct StillOverlayUniforms {
  int cursor_x;
  int cursor_y;
  uint cursor_width;
  uint cursor_height;
  uint cursor_source_width;
  uint cursor_source_height;
  uint camera_crop_x;
  uint camera_crop_y;
  uint camera_crop_width;
  uint camera_crop_height;
  int camera_frame_x;
  int camera_frame_y;
  uint camera_frame_width;
  uint camera_frame_height;
  uint camera_radius;
  uint camera_source_width;
  uint camera_source_height;
  uint camera_drop_shadow;
  uint camera_on_top;
};

static float hash(float2 position, uint seed) {
  return fract(sin(dot(position, float2(127.1, 311.7)) + float(seed) * 0.017) * 43758.5453) * 2.0 - 1.0;
}

static float noise(float2 position, uint seed) {
  float2 cell = floor(position);
  float2 local = fract(position);
  float2 eased = local * local * (3.0 - 2.0 * local);
  float top = mix(hash(cell, seed), hash(cell + float2(1.0, 0.0), seed), eased.x);
  float bottom = mix(hash(cell + float2(0.0, 1.0), seed), hash(cell + 1.0, seed), eased.x);
  return mix(top, bottom, eased.y);
}

static float fractal_noise(float2 position, uint seed) {
  return noise(position, seed) * 0.58
    + noise(position * 2.07 + float2(11.3, -4.9), seed ^ 0x68bc21eb) * 0.28
    + noise(position * 4.19 + float2(-8.7, 13.1), seed ^ 0x02e5be93) * 0.14;
}

// A stable fraction of one 8-bit step prevents smooth shadows and gradients
// from landing on the same quantisation boundary across large areas. Keeping
// it spatial (rather than changing it every frame) avoids shimmer and needless
// bitrate while making the RGBA preview and encoded canvas use the same image.
static float3 output_dither(float3 colour, float2 point) {
  // A full 8-bit step of spatial noise: every fractional gradient value
  // crosses its quantisation threshold somewhere nearby, which is what keeps
  // smooth gradients from banding after the encoder quantises them. The
  // offset stays positive so no pixel moves more than one step from its
  // undithered value.
  float value = hash(point, 0x9e3779b9) * (1.0 / 255.0);
  return clamp(colour + value, 0.0, 1.0);
}

static bool rounded_pixel_visible(float2 point, float2 size, float radius) {
  if (radius <= 0.0) return true;
  float2 edge = min(point, size - point);
  float2 corner = max(float2(0.0), radius - edge);
  return length(corner) <= radius;
}

static float rounded_box_distance(float2 point, float2 size, float radius) {
  float2 half_size = size * 0.5;
  float2 offset = abs(point - half_size) - (half_size - radius);
  return length(max(offset, 0.0)) + min(max(offset.x, offset.y), 0.0) - radius;
}

static float shadow_sigma(float2 size) {
  return clamp(min(size.x, size.y) * 0.055, 10.0, 110.0);
}

static float margin_capped_sigma(float2 origin, float2 size, float2 canvas,
                                 float base_sigma) {
  // The blur must fit the background actually visible around the object: a
  // near-full-canvas image would otherwise sit inside its own falloff and the
  // margins read as a dark tint instead of a shadow.
  float2 lower = origin;
  float2 upper = canvas - (origin + size);
  float margin = max(0.0, min(min(lower.x, lower.y), min(upper.x, upper.y)));
  return min(base_sigma, margin * 0.45);
}

static float soft_shadow(float2 point, float2 size, float radius,
                         float sigma, float opacity) {
  // CSS box-shadow uses a softer, offset falloff than a centred Gaussian.
  // Moving the sampled box down avoids a dark outline around every edge and
  // gives the recording and camera the same lifted appearance as the preview.
  float2 offset_point = point - float2(0.0, sigma * 0.35);
  float distance = max(0.0, rounded_box_distance(offset_point, size, radius));
  return distance < sigma * 4.0
    ? exp(-(distance * distance) / (2.0 * sigma * sigma)) * opacity
    : 0.0;
}

static float visible_foreground_shadow(
    float2 point, float2 crop_origin, float2 crop_size, float crop_radius,
    float2 image_origin, float2 image_size, float sigma, float opacity) {
  float2 offset_point = point - float2(0.0, sigma * 0.35);
  float crop_distance = rounded_box_distance(
    offset_point - crop_origin, crop_size, crop_radius);
  float image_distance = rounded_box_distance(
    offset_point - image_origin, image_size, 0.0);
  float distance = max(0.0, max(crop_distance, image_distance));
  return distance < sigma * 4.0
    ? exp(-(distance * distance) / (2.0 * sigma * sigma)) * opacity
    : 0.0;
}

static float visible_foreground_sigma(
    float2 crop_origin, float2 crop_size, float2 image_origin,
    float2 image_size, float2 canvas) {
  float2 origin = max(crop_origin, image_origin);
  float2 end = min(crop_origin + crop_size, image_origin + image_size);
  float2 size = max(end - origin, 0.0);
  return min(size.x, size.y) > 0.0
    ? margin_capped_sigma(origin, size, canvas, shadow_sigma(size))
    : 0.0;
}

static float3 mesh_pixel(float2 point, float2 dimensions,
                         constant CanvasUniforms &u, float seconds) {
  float shortest = min(dimensions.x, dimensions.y);
  float frequency = 3.5 / shortest;
  float phase = seconds * 0.28;
  float2 drift = float2(sin(phase), cos(phase * 0.83)) * shortest * 0.012;
  float2 warped_point = point + drift;
  float warp_scale = shortest * u.mesh_warp_percent / 100.0;
  float2 warp = float2(
    fractal_noise(warped_point * frequency + phase * 0.035, u.mesh_seed),
    fractal_noise(warped_point * frequency + float2(19.7, -7.3) - phase * 0.03,
                  u.mesh_seed ^ 0xa511e9b3)
  ) * warp_scale;
  float2 aspect = dimensions / shortest;
  float3 weighted = u.mesh_colors[u.mesh_point_count].rgb * 0.18;
  float total = 0.18;
  for (uint index = 0; index < u.mesh_point_count; ++index) {
    float4 first = u.mesh_points[index * 2];
    float4 second = u.mesh_points[index * 2 + 1];
    float local_phase = phase + float(index) * 1.73;
    float2 animated_center = first.xy + float2(sin(local_phase), cos(local_phase * 0.91)) * 0.012;
    float2 delta = (point + warp) / shortest - animated_center * aspect;
    float2 rotated = float2(delta.x * second.x + delta.y * second.y,
                            -delta.x * second.y + delta.y * second.x);
    float distance = length(rotated / max(first.zw, float2(0.01)));
    float weight = 1.0 / (pow(max(distance, 0.025), 3.5) + 0.012);
    weighted += u.mesh_colors[index].rgb * weight;
    total += weight;
  }
  float depth = fractal_noise((point + drift) * frequency * 0.7,
                              u.mesh_seed ^ 0xd1b54a35) * 13.0 / 255.0;
  return clamp(weighted / total + depth, 0.0, 1.0);
}

static float3 yuv_to_rgb(float y, float2 uv) {
  float adjusted_y = max(0.0, (y - 16.0 / 255.0) * (255.0 / 219.0));
  float2 adjusted_uv = uv - 0.5;
  return clamp(float3(
    adjusted_y + 1.5748 * adjusted_uv.y,
    adjusted_y - 0.1873 * adjusted_uv.x - 0.4681 * adjusted_uv.y,
    adjusted_y + 1.8556 * adjusted_uv.x), 0.0, 1.0);
}

static float3 source_pixel(texture2d<float, access::sample> source_y,
                           texture2d<float, access::sample> source_uv,
                           float2 output_point, constant CanvasUniforms &u) {
  constexpr sampler linear_sampler(coord::normalized, address::clamp_to_edge,
                                   filter::linear);
  float2 source = (output_point - float2(u.image_x, u.image_y)) /
                  float2(u.image_width, u.image_height);
  return yuv_to_rgb(source_y.sample(linear_sampler, source).r,
                    source_uv.sample(linear_sampler, source).rg);
}

static float4 rgba_source_pixel(const device uchar4 *source,
                                uint source_width, uint source_height,
                                float2 output_point,
                                constant CanvasUniforms &u) {
  float2 coordinate = (output_point - float2(u.image_x, u.image_y)) /
                      float2(u.image_width, u.image_height);
  float2 pixel = clamp(coordinate * float2(source_width, source_height) - 0.5,
                       0.0,
                       float2(source_width - 1, source_height - 1));
  uint2 first = uint2(floor(pixel));
  uint2 second = min(first + 1, uint2(source_width - 1, source_height - 1));
  float2 amount = fract(pixel);
  float4 top = mix(float4(source[first.y * source_width + first.x]) / 255.0,
                   float4(source[first.y * source_width + second.x]) / 255.0,
                   amount.x);
  float4 bottom = mix(float4(source[second.y * source_width + first.x]) / 255.0,
                      float4(source[second.y * source_width + second.x]) / 255.0,
                      amount.x);
  return mix(top, bottom, amount.y);
}

static float rounded_coverage(float2 point, float2 size, float radius) {
  // Half-pixel smoothing over the signed distance antialiases rounded corners
  // instead of the old binary inside test. A zero radius stays a hard edge:
  // axis-aligned boundaries are already pixel-exact and smoothing them would
  // darken the outermost row.
  float distance = rounded_box_distance(point, size, radius);
  if (radius <= 0.0) return distance < 0.0 ? 1.0 : 0.0;
  return 1.0 - smoothstep(-0.75, 0.75, distance);
}

static float4 canvas_rgba_pixel(const device uchar4 *source,
                                uint source_width, uint source_height,
                                float2 point, float2 dimensions,
                                constant CanvasUniforms &u, float seconds) {
  float3 background = u.mesh_enabled != 0
    ? mesh_pixel(point, dimensions, u, seconds)
    : u.background_color.rgb;
  float background_alpha = u.foreground_only != 0 ? 0.0 : 1.0;
  float2 crop_point = point - float2(u.crop_x, u.crop_y);
  float2 crop_size = float2(u.crop_width, u.crop_height);
  float2 crop_origin = float2(u.crop_x, u.crop_y);
  float2 image_origin = float2(u.image_x, u.image_y);
  float2 image_size = float2(u.image_width, u.image_height);
  float crop_coverage = rounded_coverage(crop_point, crop_size, float(u.radius));
  float2 image_point = point - image_origin;
  float image_coverage = crop_coverage *
    rounded_coverage(image_point, image_size, 0.0);
  if (u.drop_shadow != 0) {
    float sigma = visible_foreground_sigma(
      crop_origin, crop_size, image_origin, image_size, dimensions);
    if (sigma > 1.0) {
      float shadow = visible_foreground_shadow(
        point, crop_origin, crop_size, float(u.radius),
        image_origin, image_size, sigma, 0.14);
      if (u.foreground_only != 0) {
        background = float3(0.0);
        background_alpha = shadow * (1.0 - image_coverage);
      } else {
        background *= 1.0 - shadow * (1.0 - image_coverage);
      }
    }
  }
  float4 result = float4(background * background_alpha, background_alpha);
  // The source only exists inside its placed rect: a crop reaching past it
  // (a 4:5 or 9:16 canvas around a wide capture) must show background there,
  // not clamp-to-edge smears of the outermost source pixels.
  if (image_coverage > 0.0) {
    float4 video = rgba_source_pixel(source, source_width, source_height, point, u);
    float source_alpha = video.a * image_coverage;
    result.rgb = video.rgb * source_alpha + result.rgb * (1.0 - source_alpha);
    result.a = source_alpha + result.a * (1.0 - source_alpha);
  }
  return result;
}

static float4 overlay_canvas_foreground_rgba(
    float4 result, const device uchar4 *source, uint source_width,
    uint source_height, float2 point, float2 dimensions,
    constant CanvasUniforms &u) {
  float2 crop_origin = float2(u.crop_x, u.crop_y);
  float2 crop_size = float2(u.crop_width, u.crop_height);
  float2 image_origin = float2(u.image_x, u.image_y);
  float2 image_size = float2(u.image_width, u.image_height);
  float crop_coverage = rounded_coverage(
    point - crop_origin, crop_size, float(u.radius));
  float image_coverage = crop_coverage * rounded_coverage(
    point - image_origin, image_size, 0.0);
  if (u.drop_shadow != 0) {
    float sigma = visible_foreground_sigma(
      crop_origin, crop_size, image_origin, image_size, dimensions);
    if (sigma > 1.0) {
      float shadow = visible_foreground_shadow(
        point, crop_origin, crop_size, float(u.radius), image_origin,
        image_size, sigma, 0.14);
      result.rgb *= 1.0 - shadow * (1.0 - image_coverage);
    }
  }
  if (image_coverage > 0.0) {
    float4 video = rgba_source_pixel(
      source, source_width, source_height, point, u);
    float source_alpha = video.a * image_coverage;
    result.rgb = video.rgb * source_alpha + result.rgb * (1.0 - source_alpha);
    result.a = source_alpha + result.a * (1.0 - source_alpha);
  }
  return result;
}

kernel void compose_canvas_rgba(
    const device uchar4 *source [[buffer(0)]],
    device uchar4 *output [[buffer(1)]],
    constant CanvasUniforms &u [[buffer(2)]],
    constant uint2 &source_dimensions [[buffer(3)]],
    constant float &seconds [[buffer(4)]],
    const device uchar4 *cursor [[buffer(5)]],
    const device uchar4 *camera [[buffer(6)]],
    constant StillOverlayUniforms &overlay [[buffer(7)]],
    uint2 gid [[thread_position_in_grid]],
    uint2 dimensions [[threads_per_grid]]) {
  if (any(gid >= dimensions)) return;
  float4 rgba = canvas_rgba_pixel(
    source, source_dimensions.x, source_dimensions.y, float2(gid) + 0.5,
    float2(dimensions), u, seconds);
  int2 cursor_point = int2(gid) - int2(overlay.cursor_x, overlay.cursor_y);
  if (overlay.cursor_width > 0 && cursor_point.x >= 0 && cursor_point.y >= 0 &&
      cursor_point.x < int(overlay.cursor_width) &&
      cursor_point.y < int(overlay.cursor_height)) {
    bool cursor_visible = true;
    if (u.clip_cursor_at_video_edge != 0) {
      float2 crop_point = float2(gid) + 0.5 - float2(u.crop_x, u.crop_y);
      float2 crop_size = float2(u.crop_width, u.crop_height);
      cursor_visible = all(crop_point >= 0.0) && all(crop_point < crop_size) &&
        rounded_pixel_visible(crop_point, crop_size, float(u.radius));
    }
    if (cursor_visible) {
      uint2 cursor_source = min(uint2(
        float2(cursor_point) / float2(overlay.cursor_width, overlay.cursor_height) *
        float2(overlay.cursor_source_width, overlay.cursor_source_height)),
        uint2(overlay.cursor_source_width - 1, overlay.cursor_source_height - 1));
      float4 cursor_pixel = float4(cursor[
        cursor_source.y * overlay.cursor_source_width + cursor_source.x]) / 255.0;
      rgba = mix(rgba, cursor_pixel, cursor_pixel.a);
    }
  }
  float2 camera_point = float2(gid) -
    float2(overlay.camera_frame_x, overlay.camera_frame_y);
  float2 camera_size = float2(
    overlay.camera_frame_width, overlay.camera_frame_height);
  float camera_coverage = overlay.camera_frame_width > 0
    ? rounded_coverage(camera_point, camera_size, float(overlay.camera_radius))
    : 0.0;
  if (overlay.camera_frame_width > 0 && overlay.camera_drop_shadow != 0) {
    float camera_sigma = margin_capped_sigma(
      float2(overlay.camera_frame_x, overlay.camera_frame_y), camera_size,
      float2(dimensions), shadow_sigma(camera_size));
    if (camera_sigma > 1.0) {
      float camera_shadow = soft_shadow(
        camera_point, camera_size, float(overlay.camera_radius),
        camera_sigma, 0.14);
      // The shadow belongs to the area outside the camera frame. Applying it
      // to the accumulated canvas before compositing the camera tints the
      // camera itself as well, especially when the overlay is large.
      rgba.rgb *= 1.0 - camera_shadow * (1.0 - camera_coverage);
    }
  }
  if (camera_coverage > 0.0) {
    float2 source_point = float2(overlay.camera_crop_x, overlay.camera_crop_y) +
      clamp(camera_point, float2(0.0), camera_size) / camera_size *
      float2(overlay.camera_crop_width, overlay.camera_crop_height);
    uint2 camera_pixel = min(uint2(source_point), uint2(
      overlay.camera_source_width - 1, overlay.camera_source_height - 1));
    float4 camera_rgba = float4(camera[
      camera_pixel.y * overlay.camera_source_width + camera_pixel.x]) / 255.0;
    rgba = mix(rgba, camera_rgba, camera_coverage * camera_rgba.a);
  }
  if (overlay.camera_frame_width > 0 && overlay.camera_on_top == 0) {
    rgba = overlay_canvas_foreground_rgba(
      rgba, source, source_dimensions.x, source_dimensions.y,
      float2(gid) + 0.5, float2(dimensions), u);
    if (overlay.cursor_width > 0 && cursor_point.x >= 0 && cursor_point.y >= 0 &&
        cursor_point.x < int(overlay.cursor_width) &&
        cursor_point.y < int(overlay.cursor_height)) {
      bool cursor_visible = true;
      if (u.clip_cursor_at_video_edge != 0) {
        float2 crop_point = float2(gid) + 0.5 - float2(u.crop_x, u.crop_y);
        float2 crop_size = float2(u.crop_width, u.crop_height);
        cursor_visible = all(crop_point >= 0.0) && all(crop_point < crop_size) &&
          rounded_pixel_visible(crop_point, crop_size, float(u.radius));
      }
      if (cursor_visible) {
        uint2 cursor_source = min(uint2(
          float2(cursor_point) / float2(overlay.cursor_width, overlay.cursor_height) *
          float2(overlay.cursor_source_width, overlay.cursor_source_height)),
          uint2(overlay.cursor_source_width - 1, overlay.cursor_source_height - 1));
        float4 cursor_pixel = float4(cursor[
          cursor_source.y * overlay.cursor_source_width + cursor_source.x]) / 255.0;
        rgba = mix(rgba, cursor_pixel, cursor_pixel.a);
      }
    }
  }
  float canvas_coverage = rounded_coverage(
    float2(gid) + 0.5, float2(dimensions), float(u.background_radius));
  if (u.foreground_only == 0) rgba.rgb = output_dither(rgba.rgb, float2(gid));
  rgba.rgb *= canvas_coverage;
  rgba.a = u.foreground_only != 0 || u.transparent_background != 0
    ? rgba.a * canvas_coverage : 1.0;
  output[gid.y * dimensions.x + gid.x] = uchar4(
    clamp(rgba, 0.0, 1.0) * 255.0 + 0.5);
}

kernel void present_canvas_rgba(
    const device uchar4 *source [[buffer(0)]],
    constant CanvasUniforms &u [[buffer(1)]],
    constant uint2 &source_dimensions [[buffer(2)]],
    constant float &seconds [[buffer(3)]],
    const device uchar4 *cursor [[buffer(4)]],
    const device uchar4 *camera [[buffer(5)]],
    constant StillOverlayUniforms &overlay [[buffer(6)]],
    texture2d<float, access::write> output [[texture(0)]],
    uint2 gid [[thread_position_in_grid]]) {
  uint2 dimensions(output.get_width(), output.get_height());
  if (any(gid >= dimensions)) return;
  float2 canvas_dimensions = float2(dimensions);
  float2 point = float2(gid) + 0.5;
  float4 rgba = canvas_rgba_pixel(source, source_dimensions.x,
                                  source_dimensions.y, point,
                                  canvas_dimensions, u, seconds);
  int2 cursor_point = int2(floor(point)) - int2(overlay.cursor_x, overlay.cursor_y);
  if (overlay.cursor_width > 0 && cursor_point.x >= 0 && cursor_point.y >= 0 &&
      cursor_point.x < int(overlay.cursor_width) &&
      cursor_point.y < int(overlay.cursor_height)) {
    bool cursor_visible = true;
    if (u.clip_cursor_at_video_edge != 0) {
      float2 crop_point = point - float2(u.crop_x, u.crop_y);
      float2 crop_size = float2(u.crop_width, u.crop_height);
      cursor_visible = all(crop_point >= 0.0) && all(crop_point < crop_size) &&
        rounded_pixel_visible(crop_point, crop_size, float(u.radius));
    }
    if (cursor_visible) {
      uint2 cursor_source = min(uint2(
        float2(cursor_point) / float2(overlay.cursor_width, overlay.cursor_height) *
        float2(overlay.cursor_source_width, overlay.cursor_source_height)),
        uint2(overlay.cursor_source_width - 1, overlay.cursor_source_height - 1));
      float4 cursor_pixel = float4(cursor[
        cursor_source.y * overlay.cursor_source_width + cursor_source.x]) / 255.0;
      rgba = mix(rgba, cursor_pixel, cursor_pixel.a);
    }
  }
  float2 camera_point = point -
    float2(overlay.camera_frame_x, overlay.camera_frame_y);
  float2 camera_size = float2(
    overlay.camera_frame_width, overlay.camera_frame_height);
  float camera_coverage = overlay.camera_frame_width > 0
    ? rounded_coverage(camera_point, camera_size, float(overlay.camera_radius))
    : 0.0;
  if (overlay.camera_frame_width > 0 && overlay.camera_drop_shadow != 0) {
    float camera_sigma = margin_capped_sigma(
      float2(overlay.camera_frame_x, overlay.camera_frame_y), camera_size,
      float2(dimensions), shadow_sigma(camera_size));
    if (camera_sigma > 1.0) {
      float camera_shadow = soft_shadow(
        camera_point, camera_size, float(overlay.camera_radius),
        camera_sigma, 0.14);
      rgba.rgb *= 1.0 - camera_shadow * (1.0 - camera_coverage);
    }
  }
  if (camera_coverage > 0.0) {
    float2 source_point = float2(overlay.camera_crop_x, overlay.camera_crop_y) +
      clamp(camera_point, float2(0.0), camera_size) / camera_size *
      float2(overlay.camera_crop_width, overlay.camera_crop_height);
    uint2 camera_pixel = min(uint2(source_point), uint2(
      overlay.camera_source_width - 1, overlay.camera_source_height - 1));
    float4 camera_rgba = float4(camera[
      camera_pixel.y * overlay.camera_source_width + camera_pixel.x]) / 255.0;
    rgba = mix(rgba, camera_rgba, camera_coverage * camera_rgba.a);
  }
  if (overlay.camera_frame_width > 0 && overlay.camera_on_top == 0) {
    rgba = overlay_canvas_foreground_rgba(
      rgba, source, source_dimensions.x, source_dimensions.y, point,
      canvas_dimensions, u);
    if (overlay.cursor_width > 0 && cursor_point.x >= 0 && cursor_point.y >= 0 &&
        cursor_point.x < int(overlay.cursor_width) &&
        cursor_point.y < int(overlay.cursor_height)) {
      bool cursor_visible = true;
      if (u.clip_cursor_at_video_edge != 0) {
        float2 crop_point = point - float2(u.crop_x, u.crop_y);
        float2 crop_size = float2(u.crop_width, u.crop_height);
        cursor_visible = all(crop_point >= 0.0) && all(crop_point < crop_size) &&
          rounded_pixel_visible(crop_point, crop_size, float(u.radius));
      }
      if (cursor_visible) {
        uint2 cursor_source = min(uint2(
          float2(cursor_point) / float2(overlay.cursor_width, overlay.cursor_height) *
          float2(overlay.cursor_source_width, overlay.cursor_source_height)),
          uint2(overlay.cursor_source_width - 1, overlay.cursor_source_height - 1));
        float4 cursor_pixel = float4(cursor[
          cursor_source.y * overlay.cursor_source_width + cursor_source.x]) / 255.0;
        rgba = mix(rgba, cursor_pixel, cursor_pixel.a);
      }
    }
  }
  float canvas_coverage = rounded_coverage(
    point, canvas_dimensions, float(u.background_radius));
  if (u.foreground_only == 0) rgba.rgb = output_dither(rgba.rgb, float2(gid));
  rgba.rgb *= canvas_coverage;
  rgba.a = u.foreground_only != 0 || u.transparent_background != 0
    ? rgba.a * canvas_coverage : 1.0;
  output.write(clamp(rgba, 0.0, 1.0), gid);
}

struct WorkspacePlacement {
  int x;
  int y;
  uint width;
  uint height;
};

struct WorkspaceMagnifier {
  uint active;
  uint pane_index;
  uint layer_id;
  uint sample_camera;
  uint edges;
  uint light_mode;
  float sample_u;
  float sample_v;
  int box_x;
  int box_y;
  uint box_width;
  uint box_height;
};

// Clear and layer composition are separate kernels so every workspace layer
// can be submitted to one command buffer while preserving foreground-over
// ordering in a single read/write drawable-sized texture.
kernel void workspace_clear(
    texture2d<float, access::write> output [[texture(0)]],
    uint2 gid [[thread_position_in_grid]]) {
  if (any(gid >= uint2(output.get_width(), output.get_height()))) return;
  output.write(float4(0.0), gid);
}

kernel void workspace_layer(
    const device uchar4 *source [[buffer(0)]],
    texture2d<float, access::read_write> output [[texture(0)]],
    constant CanvasUniforms &u [[buffer(1)]],
    constant uint2 &source_dimensions [[buffer(2)]],
    constant WorkspacePlacement &placement [[buffer(3)]],
    constant uint &first_layer [[buffer(4)]],
    constant uint2 &logical_dimensions [[buffer(5)]],
    const device uchar4 *cursor [[buffer(6)]],
    const device uchar4 *camera [[buffer(7)]],
    constant StillOverlayUniforms &overlay [[buffer(8)]],
    constant float &seconds [[buffer(9)]],
    uint2 gid [[thread_position_in_grid]]) {
  uint2 dimensions(output.get_width(), output.get_height());
  if (any(gid >= dimensions) || placement.width == 0 || placement.height == 0)
    return;
  float2 global_point = float2(gid) + 0.5;
  float2 local = global_point - float2(placement.x, placement.y);
  if (any(local < 0.0) || local.x >= float(placement.width) ||
      local.y >= float(placement.height)) return;
  float2 canvas_dimensions = float2(logical_dimensions);
  float2 canvas_point = local / float2(placement.width, placement.height) *
                        canvas_dimensions;
  float canvas_coverage = rounded_coverage(
      canvas_point, canvas_dimensions, float(u.background_radius));
  float4 existing = output.read(gid);
  float4 rgba;
  if (first_layer != 0 || u.foreground_only == 0) {
    rgba = canvas_rgba_pixel(source, source_dimensions.x, source_dimensions.y,
                             canvas_point, canvas_dimensions, u, seconds);
  } else {
    rgba = overlay_canvas_foreground_rgba(
        existing, source, source_dimensions.x, source_dimensions.y,
        canvas_point, canvas_dimensions, u);
  }
  int2 cursor_point = int2(canvas_point) - int2(overlay.cursor_x, overlay.cursor_y);
  if (overlay.cursor_width > 0 && cursor_point.x >= 0 && cursor_point.y >= 0 &&
      cursor_point.x < int(overlay.cursor_width) &&
      cursor_point.y < int(overlay.cursor_height)) {
    uint2 cursor_source = min(uint2(
      float2(cursor_point) / float2(overlay.cursor_width, overlay.cursor_height) *
      float2(overlay.cursor_source_width, overlay.cursor_source_height)),
      uint2(overlay.cursor_source_width - 1, overlay.cursor_source_height - 1));
    bool visible = true;
    if (u.clip_cursor_at_video_edge != 0) {
      float2 crop_point = canvas_point - float2(u.crop_x, u.crop_y);
      float2 crop_size = float2(u.crop_width, u.crop_height);
      visible = all(crop_point >= 0.0) && all(crop_point < crop_size) &&
        rounded_pixel_visible(crop_point, crop_size, float(u.radius));
    }
    if (visible) {
      float4 pixel = float4(cursor[cursor_source.y * overlay.cursor_source_width +
                                   cursor_source.x]) / 255.0;
      rgba = mix(rgba, pixel, pixel.a);
    }
  }
  float2 camera_point = canvas_point - float2(overlay.camera_frame_x,
                                                overlay.camera_frame_y);
  float2 camera_size = float2(overlay.camera_frame_width,
                              overlay.camera_frame_height);
  float camera_coverage = overlay.camera_frame_width > 0
    ? rounded_coverage(camera_point, camera_size, float(overlay.camera_radius)) : 0.0;
  if (camera_coverage > 0.0) {
    float2 source_point = float2(overlay.camera_crop_x, overlay.camera_crop_y) +
      clamp(camera_point, float2(0.0), camera_size) / camera_size *
      float2(overlay.camera_crop_width, overlay.camera_crop_height);
    uint2 camera_source = min(uint2(source_point),
      uint2(overlay.camera_source_width - 1, overlay.camera_source_height - 1));
    float4 pixel = float4(camera[camera_source.y * overlay.camera_source_width +
                                 camera_source.x]) / 255.0;
    rgba = mix(rgba, pixel, camera_coverage * pixel.a);
  }
  if (overlay.camera_frame_width > 0 && overlay.camera_drop_shadow != 0) {
    float sigma = margin_capped_sigma(
      float2(overlay.camera_frame_x, overlay.camera_frame_y), camera_size,
      canvas_dimensions, shadow_sigma(camera_size));
    if (sigma > 1.0) {
      float shadow = soft_shadow(camera_point, camera_size,
        float(overlay.camera_radius), sigma, 0.14);
      rgba.rgb *= 1.0 - shadow * (1.0 - camera_coverage);
    }
  }
  if (overlay.camera_frame_width > 0 && overlay.camera_on_top == 0) {
    rgba = overlay_canvas_foreground_rgba(
      rgba, source, source_dimensions.x, source_dimensions.y,
      canvas_point, canvas_dimensions, u);
  }
  if (u.foreground_only == 0) rgba.rgb = output_dither(rgba.rgb, global_point);
  rgba.rgb *= canvas_coverage;
  rgba.a = u.foreground_only != 0 || u.transparent_background != 0
    ? rgba.a * canvas_coverage : 1.0;
  output.write(rgba, gid);
}

kernel void workspace_magnifier(
    const device uchar4 *source [[buffer(0)]],
    texture2d<float, access::read_write> output [[texture(0)]],
    constant uint2 &source_dimensions [[buffer(1)]],
    constant WorkspaceMagnifier &magnifier [[buffer(2)]],
    uint2 gid [[thread_position_in_grid]]) {
  if (magnifier.active == 0 || gid.x >= magnifier.box_width ||
      gid.y >= magnifier.box_height || any(source_dimensions == 0)) return;
  int2 output_point = int2(magnifier.box_x, magnifier.box_y) + int2(gid);
  if (any(output_point < 0) || output_point.x >= int(output.get_width()) ||
      output_point.y >= int(output.get_height())) return;
  float2 box_size = float2(magnifier.box_width, magnifier.box_height);
  float2 local = float2(gid) + 0.5;
  float radius = 4.0;
  float2 half_size = box_size * 0.5;
  float2 rounded = abs(local - half_size) - (half_size - radius);
  float distance = length(max(rounded, 0.0)) +
                   min(max(rounded.x, rounded.y), 0.0) - radius;
  if (distance > 0.0) return;
  float2 source_center = float2(magnifier.sample_u, magnifier.sample_v) *
                         float2(source_dimensions);
  float2 source_point = source_center +
      (local / box_size - 0.5) * 40.0;
  uint2 sample_point = min(uint2(max(floor(source_point), 0.0)),
                           source_dimensions - 1);
  float4 pixel = float4(source[sample_point.y * source_dimensions.x +
                               sample_point.x]) / 255.0;
  bool shade = ((magnifier.edges & 1u) != 0u && local.x < half_size.x) ||
               ((magnifier.edges & 2u) != 0u && local.x >= half_size.x) ||
               ((magnifier.edges & 4u) != 0u && local.y < half_size.y) ||
               ((magnifier.edges & 8u) != 0u && local.y >= half_size.y);
  if (shade) {
    float3 shade_color = magnifier.light_mode != 0
        ? float3(0.0) : float3(1.0);
    pixel.rgb = mix(pixel.rgb, shade_color, 0.1);
  }
  if (distance > -1.0) pixel = float4(0.15, 0.15, 0.16, 1.0);
  output.write(pixel, uint2(output_point));
}

kernel void unpack_preview_bgra(
    texture2d<float, access::read> source [[texture(0)]],
    device uchar4 *output [[buffer(0)]],
    uint2 gid [[thread_position_in_grid]]) {
  uint2 dimensions(source.get_width(), source.get_height());
  if (any(gid >= dimensions)) return;
  output[gid.y * dimensions.x + gid.x] = uchar4(
    clamp(source.read(gid), 0.0, 1.0) * 255.0 + 0.5);
}

static float3 canvas_pixel(texture2d<float, access::sample> source_y,
                           texture2d<float, access::sample> source_uv,
                           float2 point, float2 dimensions,
                           constant CanvasUniforms &u, float seconds) {
  float3 background = u.mesh_enabled != 0
    ? mesh_pixel(point, dimensions, u, seconds)
    : u.background_color.rgb;
  float canvas_coverage = rounded_coverage(
    point, dimensions, float(u.background_radius));
  float2 crop_point = point - float2(u.crop_x, u.crop_y);
  float2 crop_size = float2(u.crop_width, u.crop_height);
  float2 crop_origin = float2(u.crop_x, u.crop_y);
  float2 image_origin = float2(u.image_x, u.image_y);
  float2 image_size = float2(u.image_width, u.image_height);
  float crop_coverage = rounded_coverage(crop_point, crop_size, float(u.radius));
  float2 image_point = point - image_origin;
  float image_coverage = crop_coverage *
    rounded_coverage(image_point, image_size, 0.0);
  if (u.drop_shadow != 0) {
    float sigma = visible_foreground_sigma(
      crop_origin, crop_size, image_origin, image_size, dimensions);
    if (sigma > 1.0) {
      float shadow = visible_foreground_shadow(
        point, crop_origin, crop_size, float(u.radius),
        image_origin, image_size, sigma, 0.14);
      background *= 1.0 - shadow * (1.0 - image_coverage);
    }
  }
  if (image_coverage > 0.0)
    return mix(background, source_pixel(source_y, source_uv, point, u),
               image_coverage) * canvas_coverage;
  return background * canvas_coverage;
}

kernel void compose_canvas_luma(
    texture2d<float, access::sample> source_y [[texture(0)]],
    texture2d<float, access::sample> source_uv [[texture(1)]],
    texture2d<float, access::write> output [[texture(2)]],
    constant CanvasUniforms &u [[buffer(0)]], constant float &seconds [[buffer(1)]],
    uint2 gid [[thread_position_in_grid]]) {
  uint2 dimensions(output.get_width(), output.get_height());
  if (any(gid >= dimensions)) return;
  float3 rgb = canvas_pixel(source_y, source_uv, float2(gid) + 0.5,
                            float2(dimensions), u, seconds);
  rgb = output_dither(rgb, float2(gid));
  output.write(16.0 / 255.0 + dot(rgb, float3(0.182586, 0.614231, 0.062007)), gid);
}

kernel void compose_canvas_chroma(
    texture2d<float, access::sample> source_y [[texture(0)]],
    texture2d<float, access::sample> source_uv [[texture(1)]],
    texture2d<float, access::write> output [[texture(2)]],
    constant CanvasUniforms &u [[buffer(0)]], constant float &seconds [[buffer(1)]],
    uint2 gid [[thread_position_in_grid]]) {
  uint2 dimensions(output.get_width(), output.get_height());
  if (any(gid >= dimensions)) return;
  float3 rgb = 0.0;
  for (uint y = 0; y < 2; ++y)
    for (uint x = 0; x < 2; ++x)
      rgb += canvas_pixel(source_y, source_uv, float2(gid * 2 + uint2(x, y)) + 0.5,
                          float2(dimensions * 2), u, seconds);
  rgb *= 0.25;
  rgb = output_dither(rgb, float2(gid * 2));
  output.write(float4(
    0.5 + dot(rgb, float3(-0.100644, -0.338572, 0.439216)),
    0.5 + dot(rgb, float3(0.439216, -0.398942, -0.040274)), 0.0, 1.0), gid);
}

kernel void overlay_screen_luma(
    texture2d<float, access::sample> source_y [[texture(0)]],
    texture2d<float, access::sample> source_uv [[texture(1)]],
    texture2d<float, access::read_write> luma [[texture(2)]],
    constant CanvasUniforms &u [[buffer(0)]],
    uint2 gid [[thread_position_in_grid]]) {
  uint2 dimensions(luma.get_width(), luma.get_height());
  if (any(gid >= dimensions)) return;
  float2 point = float2(gid) + 0.5;
  float2 crop_origin = float2(u.crop_x, u.crop_y);
  float2 crop_size = float2(u.crop_width, u.crop_height);
  float2 image_origin = float2(u.image_x, u.image_y);
  float2 image_size = float2(u.image_width, u.image_height);
  float coverage = rounded_coverage(point - crop_origin, crop_size, float(u.radius)) *
    rounded_coverage(point - image_origin, image_size, 0.0);
  float existing = luma.read(gid).r;
  if (u.drop_shadow != 0) {
    float sigma = visible_foreground_sigma(
      crop_origin, crop_size, image_origin, image_size, float2(dimensions));
    if (sigma > 1.0) {
      float shadow = visible_foreground_shadow(
        point, crop_origin, crop_size, float(u.radius), image_origin,
        image_size, sigma, 0.14);
      existing = mix(existing, 16.0 / 255.0, shadow * (1.0 - coverage));
    }
  }
  if (coverage > 0.0) {
    float3 rgb = source_pixel(source_y, source_uv, point, u);
    float value = 16.0 / 255.0 + dot(rgb, float3(0.182586, 0.614231, 0.062007));
    existing = mix(existing, value, coverage);
  }
  luma.write(existing, gid);
}

kernel void overlay_screen_chroma(
    texture2d<float, access::sample> source_y [[texture(0)]],
    texture2d<float, access::sample> source_uv [[texture(1)]],
    texture2d<float, access::read_write> chroma [[texture(2)]],
    constant CanvasUniforms &u [[buffer(0)]],
    uint2 gid [[thread_position_in_grid]]) {
  uint2 dimensions(chroma.get_width(), chroma.get_height());
  if (any(gid >= dimensions)) return;
  float2 output_dimensions = float2(dimensions * 2);
  float2 crop_origin = float2(u.crop_x, u.crop_y);
  float2 crop_size = float2(u.crop_width, u.crop_height);
  float2 image_origin = float2(u.image_x, u.image_y);
  float2 image_size = float2(u.image_width, u.image_height);
  float3 rgb_sum = 0.0;
  float coverage_sum = 0.0;
  float shadow_sum = 0.0;
  for (uint y = 0; y < 2; ++y) {
    for (uint x = 0; x < 2; ++x) {
      float2 point = float2(gid * 2 + uint2(x, y)) + 0.5;
      float coverage = rounded_coverage(
        point - crop_origin, crop_size, float(u.radius)) *
        rounded_coverage(point - image_origin, image_size, 0.0);
      if (u.drop_shadow != 0) {
        float sigma = visible_foreground_sigma(
          crop_origin, crop_size, image_origin, image_size, output_dimensions);
        if (sigma > 1.0) {
          shadow_sum += visible_foreground_shadow(
            point, crop_origin, crop_size, float(u.radius), image_origin,
            image_size, sigma, 0.14) * (1.0 - coverage);
        }
      }
      if (coverage > 0.0) {
        rgb_sum += source_pixel(source_y, source_uv, point, u) * coverage;
        coverage_sum += coverage;
      }
    }
  }
  float2 existing = chroma.read(gid).rg;
  float shadow = shadow_sum * 0.25;
  if (shadow > 0.0) existing = mix(existing, float2(0.5), shadow);
  float coverage = coverage_sum * 0.25;
  if (coverage > 0.0) {
    float3 rgb = rgb_sum / max(coverage_sum, 0.0001);
    float2 value = float2(
      0.5 + dot(rgb, float3(-0.100644, -0.338572, 0.439216)),
      0.5 + dot(rgb, float3(0.439216, -0.398942, -0.040274)));
    existing = mix(existing, value, coverage);
  }
  chroma.write(float4(existing, 0.0, 1.0), gid);
}

static float4 camera_pixel(texture2d<float, access::sample> camera,
                           float2 point, constant CameraUniforms &u) {
  float coverage = rounded_coverage(
    point, float2(u.frame_width, u.frame_height), float(u.radius));
  if (coverage <= 0.0) return float4(0.0);
  constexpr sampler linear_sampler(coord::normalized, address::clamp_to_edge,
                                   filter::linear);
  float2 source = float2(u.crop_x, u.crop_y) +
                  point * float2(u.crop_width, u.crop_height) /
                      float2(u.frame_width, u.frame_height);
  return float4(camera.sample(linear_sampler,
                              source / float2(u.source_width, u.source_height)).rgb,
                coverage);
}

kernel void alpha_composite_rgba(
    const device uchar4 *base [[buffer(0)]],
    const device uchar4 *overlay [[buffer(1)]],
    device uchar4 *output [[buffer(2)]],
    uint gid [[thread_position_in_grid]],
    uint count [[threads_per_grid]]) {
  if (gid >= count) return;
  float4 below = float4(base[gid]) / 255.0;
  float4 above = float4(overlay[gid]) / 255.0;
  float inverse = 1.0 - above.a;
  float4 result = float4(
    above.rgb + below.rgb * inverse,
    above.a + below.a * inverse);
  output[gid] = uchar4(clamp(result, 0.0, 1.0) * 255.0 + 0.5);
}

kernel void overlay_camera_luma(
    texture2d<float, access::sample> camera [[texture(0)]],
    texture2d<float, access::read_write> luma [[texture(1)]],
    constant CameraUniforms &u [[buffer(0)]],
    uint2 gid [[thread_position_in_grid]]) {
  if (gid.x >= luma.get_width() || gid.y >= luma.get_height()) return;
  float2 point = float2(gid) + 0.5 - float2(u.frame_x, u.frame_y);
  float distance = rounded_box_distance(
    point, float2(u.frame_width, u.frame_height), float(u.radius));
  if (distance > 0.0) {
    float sigma = margin_capped_sigma(
      float2(u.frame_x, u.frame_y), float2(u.frame_width, u.frame_height),
      float2(luma.get_width(), luma.get_height()),
      shadow_sigma(float2(u.frame_width, u.frame_height)));
    float shadow = u.drop_shadow != 0 && sigma > 1.0
      ? soft_shadow(point, float2(u.frame_width, u.frame_height),
                    float(u.radius), sigma, 0.14)
      : 0.0;
    if (shadow > 0.0001) {
      float existing = luma.read(gid).r;
      luma.write(mix(existing, 16.0 / 255.0, shadow), gid);
    }
    return;
  }
  float4 rgba = camera_pixel(camera, point, u);
  if (rgba.a <= 0.0001) return;
  float camera_y = 16.0 / 255.0 +
                   dot(rgba.rgb, float3(0.182586, 0.614231, 0.062007));
  luma.write(mix(luma.read(gid).r, camera_y, rgba.a), gid);
}

kernel void overlay_camera_chroma(
    texture2d<float, access::sample> camera [[texture(0)]],
    texture2d<float, access::read_write> chroma [[texture(1)]],
    constant CameraUniforms &u [[buffer(0)]],
    uint2 gid [[thread_position_in_grid]]) {
  if (gid.x >= chroma.get_width() || gid.y >= chroma.get_height()) return;
  uint2 output_origin = gid * 2;
  float3 rgb_sum = 0.0;
  float alpha_sum = 0.0;
  float shadow_sum = 0.0;
  for (uint y = 0; y < 2; ++y) {
    for (uint x = 0; x < 2; ++x) {
      float2 point = float2(output_origin + uint2(x, y)) + 0.5 -
                     float2(u.frame_x, u.frame_y);
      float distance = rounded_box_distance(
        point, float2(u.frame_width, u.frame_height), float(u.radius));
      float sigma = margin_capped_sigma(
        float2(u.frame_x, u.frame_y), float2(u.frame_width, u.frame_height),
        float2(chroma.get_width() * 2, chroma.get_height() * 2),
        shadow_sigma(float2(u.frame_width, u.frame_height)));
      shadow_sum += u.drop_shadow != 0 && distance > 0.0 && sigma > 1.0
        ? soft_shadow(point, float2(u.frame_width, u.frame_height),
                      float(u.radius), sigma, 0.14)
        : 0.0;
      float4 rgba = camera_pixel(camera, point, u);
      rgb_sum += rgba.rgb * rgba.a;
      alpha_sum += rgba.a;
    }
  }
  float alpha = alpha_sum * 0.25;
  if (alpha <= 0.0001) {
    float shadow = shadow_sum * 0.25;
    if (shadow > 0.0001) {
      float2 existing = chroma.read(gid).rg;
      chroma.write(float4(mix(existing, float2(0.5), shadow), 0.0, 1.0), gid);
    }
    return;
  }
  float3 rgb = rgb_sum / max(alpha_sum, 0.0001);
  float2 camera_uv = float2(
      0.5 + dot(rgb, float3(-0.100644, -0.338572, 0.439216)),
      0.5 + dot(rgb, float3(0.439216, -0.398942, -0.040274)));
  float2 existing = chroma.read(gid).rg;
  chroma.write(float4(mix(existing, camera_uv, alpha), 0.0, 1.0), gid);
}

/// Premultiplied bilinear artwork lookup. Ports `sample_image`
/// (cursor_effects/raster.rs:152-188), which addresses texel `i` at
/// coordinate `i` and interpolates colour weighted by alpha so transparent
/// texels never bleed their colour into the edge.
static float4 artwork_texel(texture2d_array<float, access::read> images,
                            uint slice, uint bitmap_width, uint bitmap_height,
                            float x, float y) {
  float last_x = float(max(bitmap_width, 1u) - 1u);
  float last_y = float(max(bitmap_height, 1u) - 1u);
  x = clamp(x, 0.0, last_x);
  y = clamp(y, 0.0, last_y);
  uint x0 = uint(floor(x));
  uint y0 = uint(floor(y));
  uint x1 = min(x0 + 1u, uint(last_x));
  uint y1 = min(y0 + 1u, uint(last_y));
  float fraction_x = x - float(x0);
  float fraction_y = y - float(y0);
  float4 samples[4] = {
      images.read(uint2(x0, y0), slice), images.read(uint2(x1, y0), slice),
      images.read(uint2(x0, y1), slice), images.read(uint2(x1, y1), slice)};
  float weights[4] = {
      (1.0 - fraction_x) * (1.0 - fraction_y), fraction_x * (1.0 - fraction_y),
      (1.0 - fraction_x) * fraction_y, fraction_x * fraction_y};
  float alpha = 0.0;
  float3 colour = 0.0;
  for (uint index = 0; index < 4; ++index) {
    alpha += samples[index].a * weights[index];
    colour += samples[index].rgb * samples[index].a * weights[index];
  }
  if (alpha <= 0.0) return 0.0;
  return float4(colour / alpha, alpha);
}

/// One artwork sample in cursor space. Ports `CursorRaster::sample`
/// (cursor_effects/raster.rs:80-118): the output point is rotated and scaled
/// into the recorded cursor box, then mapped onto the artwork. Vector
/// fallback artwork keeps its design aspect inside that box instead.
static float4 cursor_artwork_sample(
    texture2d_array<float, access::read> images, constant OverlayUniforms &u,
    float2 point, float2 anchor) {
  float2 delta = point - anchor;
  float cosine = cos(u.cursor.rotation_radians);
  float sine = sin(u.cursor.rotation_radians);
  float2 local =
      float2(cosine * delta.x + sine * delta.y,
             -sine * delta.x + cosine * delta.y) / max(u.cursor.scale, 0.0001) +
      float2(u.cursor.hotspot_x, u.cursor.hotspot_y);
  float2 box = float2(u.cursor.width, u.cursor.height);
  if (u.artwork.clip_local_box != 0 &&
      (any(local < 0.0) || any(local >= box)))
    return 0.0;
  float2 bitmap = float2(u.artwork.width, u.artwork.height);
  if (u.artwork.use_design == 0)
    return artwork_texel(images, u.cursor.style, u.artwork.width,
                         u.artwork.height, local.x / max(box.x, 0.0001) * bitmap.x,
                         local.y / max(box.y, 0.0001) * bitmap.y);
  float2 design_size = float2(u.artwork.design_width, u.artwork.design_height);
  float artwork_scale =
      max(min(box.x / design_size.x, box.y / design_size.y), 0.01);
  float2 design = local / artwork_scale +
                  float2(u.artwork.origin_x, u.artwork.origin_y);
  if (any(design < 0.0) || any(design >= design_size)) return 0.0;
  return artwork_texel(images, u.cursor.style, u.artwork.width, u.artwork.height,
                       design.x / design_size.x * bitmap.x,
                       design.y / design_size.y * bitmap.y);
}

/// Ports `CursorRaster::sample_for_draw` (cursor_effects/raster.rs:120-145):
/// system artwork already carries an antialiased alpha edge, so only the
/// hard-edged vector fallback is supersampled over the pixel's 4x4 box.
static float4 cursor_draw_sample(texture2d_array<float, access::read> images,
                                 constant OverlayUniforms &u, float2 point,
                                 float2 anchor) {
  if (u.artwork.use_design == 0)
    return cursor_artwork_sample(images, u, point, anchor);
  const float offsets[4] = {-0.375, -0.125, 0.125, 0.375};
  float alpha = 0.0;
  float3 colour = 0.0;
  for (uint y = 0; y < 4; ++y) {
    for (uint x = 0; x < 4; ++x) {
      float4 sample = cursor_artwork_sample(
          images, u, point + float2(offsets[x], offsets[y]), anchor);
      alpha += sample.a;
      colour += sample.rgb * sample.a;
    }
  }
  if (alpha <= 0.0) return 0.0;
  return float4(colour / alpha, alpha / 16.0);
}

/// The drawn cursor at one output pixel. Ports `CursorCompositor::draw_output`
/// (cursor_effects.rs:602-643) and `draw_blurred`
/// (cursor_effects/raster.rs:239-292): exposure taps are Gaussian weighted
/// along the frame's travel, spaced no more than two output pixels apart.
static float4 cursor_pixel(texture2d_array<float, access::read> images,
                           constant OverlayUniforms &u, float2 point) {
  if (u.cursor.visible == 0 || u.artwork.width == 0 || u.artwork.height == 0)
    return 0.0;
  float2 anchor = float2(u.cursor.x, u.cursor.y);
  float2 delta = float2(u.cursor.blur_delta_x, u.cursor.blur_delta_y);
  float travel = length(delta);
  // MAX_BLUR_DISTANCE (cursor_effects.rs:36). The settings gate lives on the
  // CPU: a disabled motion blur arrives here as a zero delta.
  float distance = min(travel, 80.0);
  if (!(distance > 1.25 && travel > 0.0))
    return cursor_draw_sample(images, u, point, anchor);
  float2 direction = delta / travel;
  // motion_blur_sample_count (cursor_effects.rs:323-325).
  uint count = uint(clamp(ceil(distance / 2.0) + 1.0, 8.0, 48.0));
  float total_weight = 0.0;
  float alpha = 0.0;
  float3 colour = 0.0;
  for (uint index = 0; index < count; ++index) {
    float progress = float(index) / float(count - 1);
    float centered = (progress - 0.5) / 0.34;
    float weight = exp(-0.5 * centered * centered);
    float4 sample = cursor_artwork_sample(
        images, u, point, anchor + direction * ((progress - 0.8) * distance));
    alpha += sample.a * weight;
    colour += sample.rgb * sample.a * weight;
    total_weight += weight;
  }
  alpha /= total_weight;
  if (alpha <= 0.0) return 0.0;
  return float4(colour / (total_weight * alpha), alpha);
}

kernel void overlay_luma(texture2d_array<float, access::read> cursor [[texture(0)]],
                         texture2d<float, access::read_write> luma [[texture(1)]],
                         constant OverlayUniforms &u [[buffer(0)]],
                         uint2 gid [[thread_position_in_grid]]) {
  if (gid.x >= u.cursor_width || gid.y >= u.cursor_height) return;
  int2 output = int2(u.x, u.y) + int2(gid);
  if (output.x < 0 || output.y < 0 || output.x >= int(u.output_width) ||
      output.y >= int(u.output_height)) return;
  if (u.clip_at_video_edge != 0) {
    float2 crop_point = float2(output) + 0.5 - float2(u.crop_x, u.crop_y);
    float2 crop_size = float2(u.crop_width, u.crop_height);
    if (any(crop_point < 0.0) || any(crop_point >= crop_size) ||
        !rounded_pixel_visible(crop_point, crop_size, float(u.crop_radius))) return;
  }
  float4 rgba = cursor_pixel(cursor, u, float2(output) + 0.5);
  if (rgba.a <= 0.0001) return;
  float3 rgb = rgba.rgb;
  float cursor_y = 16.0 / 255.0 + dot(rgb, float3(0.182586, 0.614231, 0.062007));
  float existing = luma.read(uint2(output)).r;
  luma.write(mix(existing, cursor_y, rgba.a), uint2(output));
}

kernel void overlay_chroma(texture2d_array<float, access::read> cursor [[texture(0)]],
                           texture2d<float, access::read_write> chroma [[texture(1)]],
                           constant OverlayUniforms &u [[buffer(0)]],
                           uint2 gid [[thread_position_in_grid]]) {
  uint2 cursor_origin = gid * 2;
  if (cursor_origin.x >= u.cursor_width || cursor_origin.y >= u.cursor_height) return;
  int2 output_pixel = int2(u.x, u.y) + int2(cursor_origin);
  int2 output = output_pixel / 2;
  if (output.x < 0 || output.y < 0 || output.x >= int((u.output_width + 1) / 2) ||
      output.y >= int((u.output_height + 1) / 2)) return;
  if (u.clip_at_video_edge != 0) {
    float2 crop_point = float2(output_pixel) + 1.0 - float2(u.crop_x, u.crop_y);
    float2 crop_size = float2(u.crop_width, u.crop_height);
    if (any(crop_point < 0.0) || any(crop_point >= crop_size) ||
        !rounded_pixel_visible(crop_point, crop_size, float(u.crop_radius))) return;
  }
  // The chroma plane is half resolution, so one thread averages the four
  // luma-resolution cursor pixels it covers.
  float3 rgb_sum = 0.0;
  float alpha_sum = 0.0;
  for (uint y = 0; y < 2; ++y) {
    for (uint x = 0; x < 2; ++x) {
      float4 rgba =
          cursor_pixel(cursor, u, float2(output_pixel + int2(x, y)) + 0.5);
      rgb_sum += rgba.rgb * rgba.a;
      alpha_sum += rgba.a;
    }
  }
  float alpha = alpha_sum * 0.25;
  if (alpha <= 0.0001) return;
  float3 rgb = rgb_sum / max(alpha_sum, 0.0001);
  float2 cursor_uv = float2(
      0.5 + dot(rgb, float3(-0.100644, -0.338572, 0.439216)),
      0.5 + dot(rgb, float3(0.439216, -0.398942, -0.040274)));
  float2 existing = chroma.read(uint2(output)).rg;
  chroma.write(float4(mix(existing, cursor_uv, alpha), 0.0, 1.0), uint2(output));
}
)METAL";
