// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#import <AVFoundation/AVFoundation.h>
#import <Metal/Metal.h>
#import <QuartzCore/CAMetalLayer.h>
#import <VideoToolbox/VideoToolbox.h>

#import "gpu_compositor_macos.h"

typedef bool (*ScreenwideShouldCancel)(void *context);
typedef void (*ScreenwideProgress)(void *context, uint64_t position_ms);

typedef struct {
  uint64_t frame;
  int x;
  int y;
} ScreenwideCursorPosition;

typedef struct {
  int32_t x;
  int32_t y;
  uint32_t cursor_width;
  uint32_t cursor_height;
  uint32_t output_width;
  uint32_t output_height;
  int32_t crop_x;
  int32_t crop_y;
  uint32_t crop_width;
  uint32_t crop_height;
  uint32_t crop_radius;
  uint32_t clip_at_video_edge;
} ScreenwideOverlayUniforms;

typedef struct {
  uint32_t crop_x;
  uint32_t crop_y;
  uint32_t crop_width;
  uint32_t crop_height;
  uint32_t frame_x;
  uint32_t frame_y;
  uint32_t frame_width;
  uint32_t frame_height;
  uint32_t radius;
  uint32_t drop_shadow;
} ScreenwideCameraOverlay;

typedef struct {
  uint32_t crop_x;
  uint32_t crop_y;
  uint32_t crop_width;
  uint32_t crop_height;
  uint32_t frame_x;
  uint32_t frame_y;
  uint32_t frame_width;
  uint32_t frame_height;
  uint32_t radius;
  uint32_t source_width;
  uint32_t source_height;
  uint32_t drop_shadow;
} ScreenwideCameraUniforms;

static NSString *const shader_source = @R"METAL(
#include <metal_stdlib>
using namespace metal;

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
};

struct CameraUniforms {
  uint crop_x;
  uint crop_y;
  uint crop_width;
  uint crop_height;
  uint frame_x;
  uint frame_y;
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
  uint camera_frame_x;
  uint camera_frame_y;
  uint camera_frame_width;
  uint camera_frame_height;
  uint camera_radius;
  uint camera_source_width;
  uint camera_source_height;
  uint camera_drop_shadow;
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
  float3 colour = background;
  // The source only exists inside its placed rect: a crop reaching past it
  // (a 4:5 or 9:16 canvas around a wide capture) must show background there,
  // not clamp-to-edge smears of the outermost source pixels.
  if (image_coverage > 0.0) {
    float4 video = rgba_source_pixel(source, source_width, source_height, point, u);
    colour = mix(colour, video.rgb, image_coverage);
  }
  return float4(colour, 1.0);
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
  if (overlay.camera_frame_width > 0 && overlay.camera_drop_shadow != 0) {
    float camera_sigma = margin_capped_sigma(
      float2(overlay.camera_frame_x, overlay.camera_frame_y), camera_size,
      float2(dimensions), shadow_sigma(camera_size));
    if (camera_sigma > 1.0) {
      float camera_shadow = soft_shadow(
        camera_point, camera_size, float(overlay.camera_radius),
        camera_sigma, 0.14);
      rgba.rgb *= 1.0 - camera_shadow;
    }
  }
  float camera_coverage = overlay.camera_frame_width > 0
    ? rounded_coverage(camera_point, camera_size, float(overlay.camera_radius))
    : 0.0;
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
  float canvas_coverage = rounded_coverage(
    float2(gid) + 0.5, float2(dimensions), float(u.background_radius));
  rgba.rgb = output_dither(rgba.rgb, float2(gid)) * canvas_coverage;
  rgba.a = u.transparent_background != 0 ? canvas_coverage : 1.0;
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
  if (overlay.camera_frame_width > 0 && overlay.camera_drop_shadow != 0) {
    float camera_sigma = margin_capped_sigma(
      float2(overlay.camera_frame_x, overlay.camera_frame_y), camera_size,
      float2(dimensions), shadow_sigma(camera_size));
    if (camera_sigma > 1.0) {
      float camera_shadow = soft_shadow(
        camera_point, camera_size, float(overlay.camera_radius),
        camera_sigma, 0.14);
      rgba.rgb *= 1.0 - camera_shadow;
    }
  }
  float camera_coverage = overlay.camera_frame_width > 0
    ? rounded_coverage(camera_point, camera_size, float(overlay.camera_radius))
    : 0.0;
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
  float canvas_coverage = rounded_coverage(
    point, canvas_dimensions, float(u.background_radius));
  rgba.rgb = output_dither(rgba.rgb, float2(gid)) * canvas_coverage;
  rgba.a = u.transparent_background != 0 ? canvas_coverage : 1.0;
  output.write(clamp(rgba, 0.0, 1.0), gid);
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

kernel void overlay_luma(texture2d<float, access::read> cursor [[texture(0)]],
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
  float4 rgba = cursor.read(gid);
  if (rgba.a <= 0.0001) return;
  float3 rgb = rgba.rgb;
  float cursor_y = 16.0 / 255.0 + dot(rgb, float3(0.182586, 0.614231, 0.062007));
  float existing = luma.read(uint2(output)).r;
  luma.write(mix(existing, cursor_y, rgba.a), uint2(output));
}

kernel void overlay_chroma(texture2d<float, access::read> cursor [[texture(0)]],
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
  float3 rgb_sum = 0.0;
  float alpha_sum = 0.0;
  for (uint y = 0; y < 2; ++y) {
    for (uint x = 0; x < 2; ++x) {
      uint2 point = min(cursor_origin + uint2(x, y),
                        uint2(u.cursor_width - 1, u.cursor_height - 1));
      float4 rgba = cursor.read(point);
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

static int fail(char *error, size_t capacity, NSString *message) {
  if (error != NULL && capacity > 0) {
    snprintf(error, capacity, "%s",
             (message ?: @"The GPU compositor failed").UTF8String);
  }
  return 0;
}

static NSArray<AVAssetTrack *> *video_tracks(AVURLAsset *asset,
                                             NSError **error) {
  dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
  __block NSArray<AVAssetTrack *> *tracks = nil;
  __block NSError *load_error = nil;
  [asset loadTracksWithMediaType:AVMediaTypeVideo
               completionHandler:^(NSArray<AVAssetTrack *> *loaded,
                                   NSError *failure) {
                 tracks = loaded;
                 load_error = failure;
                 dispatch_semaphore_signal(semaphore);
               }];
  dispatch_semaphore_wait(semaphore, DISPATCH_TIME_FOREVER);
  if (error != NULL)
    *error = load_error;
  return tracks;
}

static NSArray<NSValue *> *read_positions(NSString *path, NSError **error) {
  NSString *contents = [NSString stringWithContentsOfFile:path
                                                 encoding:NSUTF8StringEncoding
                                                    error:error];
  if (contents == nil)
    return nil;
  NSMutableArray<NSValue *> *positions = [NSMutableArray array];
  [contents enumerateLinesUsingBlock:^(NSString *line, BOOL *stop) {
    (void)stop;
    double seconds = 0.0;
    ScreenwideCursorPosition position = {0, -100000, -100000};
    if (sscanf(line.UTF8String, "%lf overlay@cursor x %d, overlay@cursor y %d;",
               &seconds, &position.x, &position.y) == 3) {
      position.frame = (uint64_t)llround(seconds * 60.0);
      [positions
          addObject:[NSValue valueWithBytes:&position
                                   objCType:@encode(ScreenwideCursorPosition)]];
    }
  }];
  return positions;
}

static ScreenwideCursorPosition position_at(NSArray<NSValue *> *positions,
                                       NSUInteger *index, uint64_t frame) {
  while (*index + 1 < positions.count) {
    ScreenwideCursorPosition next;
    [positions[*index + 1] getValue:&next size:sizeof(next)];
    if (next.frame > frame)
      break;
    ++*index;
  }
  ScreenwideCursorPosition position = {0, -100000, -100000};
  if (positions.count > 0) {
    ScreenwideCursorPosition candidate;
    [positions[*index] getValue:&candidate size:sizeof(candidate)];
    if (candidate.frame <= frame)
      position = candidate;
  }
  return position;
}

static AVAssetReaderTrackOutput *
reader_output(AVAssetReader *reader, AVAssetTrack *track, OSType format,
              NSNumber *width, NSNumber *height, NSError **error) {
  NSMutableDictionary *settings = [@{
    (NSString *)kCVPixelBufferPixelFormatTypeKey : @(format),
    (NSString *)kCVPixelBufferMetalCompatibilityKey : @YES,
    (NSString *)kCVPixelBufferIOSurfacePropertiesKey : @{},
  } mutableCopy];
  if (width != nil)
    settings[(NSString *)kCVPixelBufferWidthKey] = width;
  if (height != nil)
    settings[(NSString *)kCVPixelBufferHeightKey] = height;
  AVAssetReaderTrackOutput *output =
      [[AVAssetReaderTrackOutput alloc] initWithTrack:track
                                       outputSettings:settings];
  output.alwaysCopiesSampleData = NO;
  if (![reader canAddOutput:output]) {
    if (error != NULL) {
      *error = [NSError errorWithDomain:@"ScreenwideGPUCompositor"
                                   code:1
                               userInfo:@{
                                 NSLocalizedDescriptionKey :
                                     @"AVFoundation rejected a GPU video reader"
                               }];
    }
    return nil;
  }
  [reader addOutput:output];
  return output;
}

static id<MTLTexture> texture(CVMetalTextureCacheRef cache,
                              CVPixelBufferRef pixels, MTLPixelFormat format,
                              size_t width, size_t height, size_t plane,
                              CVMetalTextureRef *reference) {
  CVReturn result = CVMetalTextureCacheCreateTextureFromImage(
      kCFAllocatorDefault, cache, pixels, NULL, format, width, height, plane,
      reference);
  if (result != kCVReturnSuccess || *reference == NULL)
    return nil;
  return CVMetalTextureGetTexture(*reference);
}

int screenwide_gpu_composite_cursor(const char *screen_path, const char *cursor_path,
                               const char *commands_path,
                               const char *camera_path,
                               const ScreenwideCameraOverlay *camera_overlay,
                               const ScreenwideCanvas *canvas,
                               const char *output_path, uint32_t source_width,
                               uint32_t source_height, uint32_t output_width,
                               uint32_t output_height, uint64_t bitrate,
                               void *context, ScreenwideShouldCancel should_cancel,
                               ScreenwideProgress progress, char *error_text,
                               size_t error_capacity) {
  (void)source_width;
  (void)source_height;
  @autoreleasepool {
    NSError *error = nil;
    AVURLAsset *screen_asset =
        [AVURLAsset URLAssetWithURL:[NSURL fileURLWithPath:@(screen_path)]
                            options:nil];
    AVURLAsset *cursor_asset = cursor_path == NULL
        ? nil
        : [AVURLAsset URLAssetWithURL:[NSURL fileURLWithPath:@(cursor_path)]
                              options:nil];
    AVURLAsset *camera_asset = camera_path == NULL
                                   ? nil
                                   : [AVURLAsset
                                         URLAssetWithURL:[NSURL fileURLWithPath:
                                                                      @(camera_path)]
                                                  options:nil];
    AVAssetTrack *screen_track = video_tracks(screen_asset, &error).firstObject;
    if (screen_track == nil && error != nil)
      return fail(error_text, error_capacity, error.localizedDescription);
    AVAssetTrack *cursor_track = cursor_asset == nil
        ? nil : video_tracks(cursor_asset, &error).firstObject;
    AVAssetTrack *camera_track =
        camera_asset == nil ? nil : video_tracks(camera_asset, &error).firstObject;
    if (screen_track == nil)
      return fail(error_text, error_capacity,
                  @"The GPU compositor could not find the recording track");
    if (cursor_asset != nil && cursor_track == nil)
      return fail(error_text, error_capacity,
                  @"The GPU compositor could not find the cursor track");
    if (camera_asset != nil && camera_track == nil)
      return fail(error_text, error_capacity,
                  error.localizedDescription ?:
                    @"The GPU compositor could not find the camera track");
    NSArray<NSValue *> *positions = commands_path == NULL
        ? @[] : read_positions(@(commands_path), &error);
    if (commands_path != NULL && positions == nil)
      return fail(error_text, error_capacity, error.localizedDescription);

    AVAssetReader *screen_reader =
        [[AVAssetReader alloc] initWithAsset:screen_asset error:&error];
    AVAssetReaderTrackOutput *screen_output =
        reader_output(screen_reader, screen_track,
                      kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
                      nil, nil, &error);
    if (screen_output == nil)
      return fail(error_text, error_capacity, error.localizedDescription);
    AVAssetReader *cursor_reader = cursor_asset == nil
        ? nil : [[AVAssetReader alloc] initWithAsset:cursor_asset error:&error];
    AVAssetReaderTrackOutput *cursor_output = cursor_reader == nil
        ? nil : reader_output(cursor_reader, cursor_track,
                              kCVPixelFormatType_32BGRA, nil, nil, &error);
    if (cursor_reader != nil && cursor_output == nil)
      return fail(error_text, error_capacity, error.localizedDescription);
    AVAssetReader *camera_reader =
        camera_asset == nil
            ? nil
            : [[AVAssetReader alloc] initWithAsset:camera_asset error:&error];
    AVAssetReaderTrackOutput *camera_output =
        camera_reader == nil
            ? nil
            : reader_output(camera_reader, camera_track,
                            kCVPixelFormatType_32BGRA, nil, nil, &error);
    if (camera_reader != nil && camera_output == nil)
      return fail(error_text, error_capacity, error.localizedDescription);

    NSURL *output_url = [NSURL fileURLWithPath:@(output_path)];
    [[NSFileManager defaultManager] removeItemAtURL:output_url error:nil];
    AVAssetWriter *writer = [[AVAssetWriter alloc] initWithURL:output_url
                                                      fileType:AVFileTypeMPEG4
                                                         error:&error];
    if (writer == nil)
      return fail(error_text, error_capacity, error.localizedDescription);
    writer.shouldOptimizeForNetworkUse = YES;
    float source_frame_rate = screen_track.nominalFrameRate;
    if (!isfinite(source_frame_rate) || source_frame_rate < 1.0)
      source_frame_rate = 60.0;
    NSNumber *expected_frame_rate = @((NSInteger)llround(source_frame_rate));
    NSDictionary *video_settings = @{
      AVVideoCodecKey : AVVideoCodecTypeH264,
      AVVideoWidthKey : @(output_width),
      AVVideoHeightKey : @(output_height),
      AVVideoCompressionPropertiesKey : @{
        AVVideoAverageBitRateKey : @(bitrate),
        AVVideoExpectedSourceFrameRateKey : expected_frame_rate,
        AVVideoAverageNonDroppableFrameRateKey : expected_frame_rate,
        AVVideoMaxKeyFrameIntervalKey : @(expected_frame_rate.integerValue * 4),
        AVVideoMaxKeyFrameIntervalDurationKey : @4,
        AVVideoAllowFrameReorderingKey : @NO,
        AVVideoH264EntropyModeKey : AVVideoH264EntropyModeCABAC,
        AVVideoProfileLevelKey : AVVideoProfileLevelH264HighAutoLevel,
      },
    };
    AVAssetWriterInput *writer_input =
        [[AVAssetWriterInput alloc] initWithMediaType:AVMediaTypeVideo
                                       outputSettings:video_settings];
    NSDictionary *pixel_attributes = @{
      (NSString *)kCVPixelBufferPixelFormatTypeKey :
          @(kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange),
      (NSString *)kCVPixelBufferWidthKey : @(output_width),
      (NSString *)kCVPixelBufferHeightKey : @(output_height),
      (NSString *)kCVPixelBufferMetalCompatibilityKey : @YES,
      (NSString *)kCVPixelBufferIOSurfacePropertiesKey : @{},
    };
    AVAssetWriterInputPixelBufferAdaptor *adaptor =
        [[AVAssetWriterInputPixelBufferAdaptor alloc]
               initWithAssetWriterInput:writer_input
            sourcePixelBufferAttributes:pixel_attributes];
    if (![writer canAddInput:writer_input])
      return fail(error_text, error_capacity,
                  @"AVFoundation rejected the GPU video writer");
    [writer addInput:writer_input];

    id<MTLDevice> device = MTLCreateSystemDefaultDevice();
    id<MTLLibrary> library = [device newLibraryWithSource:shader_source
                                                  options:nil
                                                    error:&error];
    id<MTLComputePipelineState> luma_pipeline =
        [device newComputePipelineStateWithFunction:
                    [library newFunctionWithName:@"overlay_luma"]
                                              error:&error];
    id<MTLComputePipelineState> chroma_pipeline =
        [device newComputePipelineStateWithFunction:
                    [library newFunctionWithName:@"overlay_chroma"]
                                              error:&error];
    id<MTLComputePipelineState> camera_luma_pipeline =
        [device newComputePipelineStateWithFunction:
                    [library newFunctionWithName:@"overlay_camera_luma"]
                                              error:&error];
    id<MTLComputePipelineState> camera_chroma_pipeline =
        [device newComputePipelineStateWithFunction:
                    [library newFunctionWithName:@"overlay_camera_chroma"]
                                              error:&error];
    id<MTLComputePipelineState> canvas_luma_pipeline =
        [device newComputePipelineStateWithFunction:
                    [library newFunctionWithName:@"compose_canvas_luma"]
                                              error:&error];
    id<MTLComputePipelineState> canvas_chroma_pipeline =
        [device newComputePipelineStateWithFunction:
                    [library newFunctionWithName:@"compose_canvas_chroma"]
                                              error:&error];
    id<MTLCommandQueue> queue = [device newCommandQueue];
    CVMetalTextureCacheRef texture_cache = NULL;
    CVMetalTextureCacheCreate(kCFAllocatorDefault, NULL, device, NULL,
                              &texture_cache);
    if (device == nil || library == nil || luma_pipeline == nil ||
        chroma_pipeline == nil || camera_luma_pipeline == nil ||
        camera_chroma_pipeline == nil || queue == nil ||
        canvas_luma_pipeline == nil || canvas_chroma_pipeline == nil ||
        texture_cache == NULL)
      return fail(error_text, error_capacity,
                  error.localizedDescription
                      ?: @"The Metal cursor shader could not be created");

    if (![screen_reader startReading] ||
        (cursor_reader != nil && ![cursor_reader startReading]) ||
        (camera_reader != nil && ![camera_reader startReading]) ||
        ![writer startWriting]) {
      CFRelease(texture_cache);
      return fail(error_text, error_capacity,
                  screen_reader.error.localizedDescription ?:
                    (cursor_reader != nil ? cursor_reader.error.localizedDescription : nil) ?:
                    (camera_reader != nil ? camera_reader.error.localizedDescription : nil) ?:
                    writer.error.localizedDescription ?:
                    @"The GPU export could not be started");
    }
    [writer startSessionAtSourceTime:kCMTimeZero];
    BOOL primed = NO;
    CMSampleBufferRef cursor_sample = NULL;
    CMSampleBufferRef next_cursor_sample = cursor_output == nil
        ? NULL : [cursor_output copyNextSampleBuffer];
    CMSampleBufferRef camera_sample = NULL;
    CMSampleBufferRef next_camera_sample = camera_output == nil
        ? NULL : [camera_output copyNextSampleBuffer];
    uint64_t cursor_frame = 0;
    uint64_t next_cursor_frame = 0;
    NSUInteger position_index = 0;
    bool cancelled = false;
    CMSampleBufferRef screen_sample = NULL;
    while ((screen_sample = [screen_output copyNextSampleBuffer]) != NULL) {
      @autoreleasepool {
        if (should_cancel != NULL && should_cancel(context)) {
          cancelled = true;
          CFRelease(screen_sample);
          break;
        }
        CMTime pts = CMSampleBufferGetPresentationTimeStamp(screen_sample);
        while (next_cursor_sample != NULL &&
               CMTimeCompare(
                   CMSampleBufferGetPresentationTimeStamp(next_cursor_sample),
                   pts) <= 0) {
          if (cursor_sample != NULL)
            CFRelease(cursor_sample);
          cursor_sample = next_cursor_sample;
          next_cursor_sample = [cursor_output copyNextSampleBuffer];
          cursor_frame = next_cursor_frame;
          ++next_cursor_frame;
        }
        while (next_camera_sample != NULL &&
               CMTimeCompare(
                   CMSampleBufferGetPresentationTimeStamp(next_camera_sample),
                   pts) <= 0) {
          if (camera_sample != NULL)
            CFRelease(camera_sample);
          camera_sample = next_camera_sample;
          next_camera_sample = [camera_output copyNextSampleBuffer];
        }
        while (!writer_input.readyForMoreMediaData) {
          if (should_cancel != NULL && should_cancel(context)) {
            cancelled = true;
            break;
          }
          [NSThread sleepForTimeInterval:0.001];
        }
        if (cancelled) {
          CFRelease(screen_sample);
          break;
        }
        CVPixelBufferRef destination = NULL;
        if (CVPixelBufferPoolCreatePixelBuffer(
                kCFAllocatorDefault, adaptor.pixelBufferPool, &destination) !=
                kCVReturnSuccess ||
            destination == NULL) {
          CFRelease(screen_sample);
          error =
              [NSError errorWithDomain:@"ScreenwideGPUCompositor"
                                  code:2
                              userInfo:@{
                                NSLocalizedDescriptionKey :
                                    @"The GPU encoder ran out of video buffers"
                              }];
          break;
        }
        CVPixelBufferRef source = CMSampleBufferGetImageBuffer(screen_sample);
        size_t source_y_width = CVPixelBufferGetWidthOfPlane(source, 0);
        size_t source_y_height = CVPixelBufferGetHeightOfPlane(source, 0);
        size_t source_uv_width = CVPixelBufferGetWidthOfPlane(source, 1);
        size_t source_uv_height = CVPixelBufferGetHeightOfPlane(source, 1);
        size_t y_width = output_width;
        size_t y_height = output_height;
        size_t uv_width = (output_width + 1) / 2;
        size_t uv_height = (output_height + 1) / 2;
        CVMetalTextureRef source_y_ref = NULL, source_uv_ref = NULL;
        CVMetalTextureRef destination_y_ref = NULL, destination_uv_ref = NULL;
        id<MTLTexture> source_y =
            texture(texture_cache, source, MTLPixelFormatR8Unorm, source_y_width,
                    source_y_height, 0, &source_y_ref);
        id<MTLTexture> source_uv =
            texture(texture_cache, source, MTLPixelFormatRG8Unorm, source_uv_width,
                    source_uv_height, 1, &source_uv_ref);
        id<MTLTexture> destination_y =
            texture(texture_cache, destination, MTLPixelFormatR8Unorm, y_width,
                    y_height, 0, &destination_y_ref);
        id<MTLTexture> destination_uv =
            texture(texture_cache, destination, MTLPixelFormatRG8Unorm,
                    uv_width, uv_height, 1, &destination_uv_ref);
        id<MTLCommandBuffer> command = [queue commandBuffer];
        float seconds = (float)CMTimeGetSeconds(pts);
        MTLSize canvas_group = MTLSizeMake(16, 16, 1);
        id<MTLComputeCommandEncoder> canvas_compute =
            [command computeCommandEncoder];
        [canvas_compute setComputePipelineState:canvas_luma_pipeline];
        [canvas_compute setTexture:source_y atIndex:0];
        [canvas_compute setTexture:source_uv atIndex:1];
        [canvas_compute setTexture:destination_y atIndex:2];
        [canvas_compute setBytes:canvas length:sizeof(*canvas) atIndex:0];
        [canvas_compute setBytes:&seconds length:sizeof(seconds) atIndex:1];
        [canvas_compute dispatchThreads:MTLSizeMake(y_width, y_height, 1)
                     threadsPerThreadgroup:canvas_group];
        [canvas_compute endEncoding];
        canvas_compute = [command computeCommandEncoder];
        [canvas_compute setComputePipelineState:canvas_chroma_pipeline];
        [canvas_compute setTexture:source_y atIndex:0];
        [canvas_compute setTexture:source_uv atIndex:1];
        [canvas_compute setTexture:destination_uv atIndex:2];
        [canvas_compute setBytes:canvas length:sizeof(*canvas) atIndex:0];
        [canvas_compute setBytes:&seconds length:sizeof(seconds) atIndex:1];
        [canvas_compute dispatchThreads:MTLSizeMake(uv_width, uv_height, 1)
                     threadsPerThreadgroup:canvas_group];
        [canvas_compute endEncoding];
        CVMetalTextureRef cursor_ref = NULL;
        if (cursor_sample != NULL) {
          CVPixelBufferRef cursor_pixels =
              CMSampleBufferGetImageBuffer(cursor_sample);
          size_t cursor_width = CVPixelBufferGetWidth(cursor_pixels);
          size_t cursor_height = CVPixelBufferGetHeight(cursor_pixels);
          id<MTLTexture> cursor_texture =
              texture(texture_cache, cursor_pixels, MTLPixelFormatBGRA8Unorm,
                      cursor_width, cursor_height, 0, &cursor_ref);
          ScreenwideCursorPosition position =
              position_at(positions, &position_index, cursor_frame);
          ScreenwideOverlayUniforms uniforms = {
              position.x,
              position.y,
              (uint32_t)cursor_width,
              (uint32_t)cursor_height,
              output_width,
              output_height,
              canvas->crop_x,
              canvas->crop_y,
              canvas->crop_width,
              canvas->crop_height,
              canvas->radius,
              canvas->clip_cursor_at_video_edge,
          };
          MTLSize group = MTLSizeMake(16, 16, 1);
          id<MTLComputeCommandEncoder> compute =
              [command computeCommandEncoder];
          [compute setComputePipelineState:luma_pipeline];
          [compute setTexture:cursor_texture atIndex:0];
          [compute setTexture:destination_y atIndex:1];
          [compute setBytes:&uniforms length:sizeof(uniforms) atIndex:0];
          [compute dispatchThreads:MTLSizeMake(cursor_width, cursor_height, 1)
              threadsPerThreadgroup:group];
          [compute endEncoding];
          compute = [command computeCommandEncoder];
          [compute setComputePipelineState:chroma_pipeline];
          [compute setTexture:cursor_texture atIndex:0];
          [compute setTexture:destination_uv atIndex:1];
          [compute setBytes:&uniforms length:sizeof(uniforms) atIndex:0];
          [compute dispatchThreads:MTLSizeMake((cursor_width + 1) / 2,
                                               (cursor_height + 1) / 2, 1)
              threadsPerThreadgroup:group];
          [compute endEncoding];
        }
        CVMetalTextureRef camera_ref = NULL;
        if (camera_sample != NULL && camera_overlay != NULL) {
          CVPixelBufferRef camera_pixels =
              CMSampleBufferGetImageBuffer(camera_sample);
          size_t camera_width = CVPixelBufferGetWidth(camera_pixels);
          size_t camera_height = CVPixelBufferGetHeight(camera_pixels);
          id<MTLTexture> camera_texture =
              texture(texture_cache, camera_pixels, MTLPixelFormatBGRA8Unorm,
                      camera_width, camera_height, 0, &camera_ref);
          ScreenwideCameraUniforms camera_uniforms = {
              camera_overlay->crop_x,
              camera_overlay->crop_y,
              camera_overlay->crop_width,
              camera_overlay->crop_height,
              camera_overlay->frame_x,
              camera_overlay->frame_y,
              camera_overlay->frame_width,
              camera_overlay->frame_height,
              camera_overlay->radius,
              (uint32_t)camera_width,
              (uint32_t)camera_height,
              camera_overlay->drop_shadow,
          };
          MTLSize camera_group = MTLSizeMake(16, 16, 1);
          id<MTLComputeCommandEncoder> camera_compute =
              [command computeCommandEncoder];
          [camera_compute setComputePipelineState:camera_luma_pipeline];
          [camera_compute setTexture:camera_texture atIndex:0];
          [camera_compute setTexture:destination_y atIndex:1];
          [camera_compute setBytes:&camera_uniforms
                            length:sizeof(camera_uniforms)
                           atIndex:0];
          [camera_compute
              dispatchThreads:MTLSizeMake(y_width, y_height, 1)
              threadsPerThreadgroup:camera_group];
          [camera_compute endEncoding];
          camera_compute = [command computeCommandEncoder];
          [camera_compute setComputePipelineState:camera_chroma_pipeline];
          [camera_compute setTexture:camera_texture atIndex:0];
          [camera_compute setTexture:destination_uv atIndex:1];
          [camera_compute setBytes:&camera_uniforms
                            length:sizeof(camera_uniforms)
                           atIndex:0];
          [camera_compute
              dispatchThreads:MTLSizeMake(uv_width, uv_height, 1)
              threadsPerThreadgroup:camera_group];
          [camera_compute endEncoding];
        }
        [command commit];
        [command waitUntilCompleted];
        if (!primed && command.status != MTLCommandBufferStatusError) {
          primed = YES;
          // Hardware rate control ramps up over its first seconds (the first
          // keyframe of an export measures at half the steady-state size).
          // Feeding the first frame repeatedly at negative timestamps warms
          // the encoder on samples the edit list trims away, so the visible
          // first frame starts at steady-state quality.
          int32_t warm_fps = (int32_t)MAX(llround(source_frame_rate), 1);
          for (int32_t warm = 45; warm >= 1; warm--) {
            while (!writer_input.isReadyForMoreMediaData)
              [NSThread sleepForTimeInterval:0.001];
            if (![adaptor appendPixelBuffer:destination
                       withPresentationTime:CMTimeMake(-warm, warm_fps)])
              break;
          }
          while (!writer_input.isReadyForMoreMediaData)
            [NSThread sleepForTimeInterval:0.001];
        }
        if (command.status == MTLCommandBufferStatusError ||
            ![adaptor appendPixelBuffer:destination withPresentationTime:pts]) {
          error = command.error ?: writer.error ?:
              [NSError errorWithDomain:@"ScreenwideGPUCompositor"
                                  code:3
                              userInfo:@{NSLocalizedDescriptionKey :
                                           @"The GPU encoder rejected a video frame"}];
        }
        if (cursor_ref != NULL)
          CFRelease(cursor_ref);
        if (camera_ref != NULL)
          CFRelease(camera_ref);
        CFRelease(source_y_ref);
        CFRelease(source_uv_ref);
        CFRelease(destination_y_ref);
        CFRelease(destination_uv_ref);
        CVPixelBufferRelease(destination);
        CFRelease(screen_sample);
        if (error != nil)
          break;
        if (progress != NULL)
          progress(context, (uint64_t)llround(CMTimeGetSeconds(pts) * 1000.0));
      }
    }
    if (cursor_sample != NULL)
      CFRelease(cursor_sample);
    if (next_cursor_sample != NULL)
      CFRelease(next_cursor_sample);
    if (camera_sample != NULL)
      CFRelease(camera_sample);
    if (next_camera_sample != NULL)
      CFRelease(next_camera_sample);
    CFRelease(texture_cache);
    if (cancelled) {
      [screen_reader cancelReading];
      if (cursor_reader != nil) [cursor_reader cancelReading];
      if (camera_reader != nil) [camera_reader cancelReading];
      [writer cancelWriting];
      [[NSFileManager defaultManager] removeItemAtURL:output_url error:nil];
      return -1;
    }
    if (error != nil || screen_reader.status == AVAssetReaderStatusFailed ||
        (cursor_reader != nil && cursor_reader.status == AVAssetReaderStatusFailed) ||
        (camera_reader != nil && camera_reader.status == AVAssetReaderStatusFailed)) {
      [writer cancelWriting];
      [[NSFileManager defaultManager] removeItemAtURL:output_url error:nil];
      return fail(error_text, error_capacity,
                  error.localizedDescription ?:
                    screen_reader.error.localizedDescription ?:
                    (cursor_reader != nil ? cursor_reader.error.localizedDescription : nil) ?:
                    (camera_reader != nil ? camera_reader.error.localizedDescription : nil) ?:
                    @"The GPU compositor could not read the recording");
    }
    [writer_input markAsFinished];
    dispatch_semaphore_t finish_semaphore = dispatch_semaphore_create(0);
    [writer finishWritingWithCompletionHandler:^{
      dispatch_semaphore_signal(finish_semaphore);
    }];
    dispatch_semaphore_wait(finish_semaphore, DISPATCH_TIME_FOREVER);
    if (writer.status != AVAssetWriterStatusCompleted) {
      [[NSFileManager defaultManager] removeItemAtURL:output_url error:nil];
      return fail(error_text, error_capacity,
                  writer.error.localizedDescription
                      ?: @"The GPU encoder could not finish the recording");
    }
    return 1;
  }
}

int screenwide_gpu_composite_still(const uint8_t *source_rgba,
                              uint32_t source_width,
                              uint32_t source_height,
                              const ScreenwideCanvas *canvas,
                              uint32_t output_width,
                              uint32_t output_height,
                              double seconds,
                              const uint8_t *cursor_rgba,
                              const uint8_t *camera_rgba,
                              const ScreenwideStillOverlay *overlay,
                              uint8_t *output_rgba,
                              char *error_text,
                              size_t error_capacity) {
  @autoreleasepool {
    if (source_rgba == NULL || output_rgba == NULL || canvas == NULL ||
        source_width == 0 || source_height == 0 ||
        output_width == 0 || output_height == 0) {
      return fail(error_text, error_capacity,
                  @"The GPU still compositor received invalid pixels");
    }
    static id<MTLDevice> device;
    static id<MTLComputePipelineState> pipeline;
    static id<MTLCommandQueue> queue;
    static NSString *initialization_error;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
      NSError *error = nil;
      device = MTLCreateSystemDefaultDevice();
      id<MTLLibrary> library = [device newLibraryWithSource:shader_source
                                                    options:nil
                                                      error:&error];
      id<MTLFunction> function =
          [library newFunctionWithName:@"compose_canvas_rgba"];
      pipeline =
          [device newComputePipelineStateWithFunction:function error:&error];
      queue = [device newCommandQueue];
      initialization_error = error.localizedDescription;
    });
    if (device == nil || pipeline == nil || queue == nil) {
      return fail(error_text, error_capacity,
                  initialization_error ?:
                    @"The Metal still compositor could not be created");
    }
    NSUInteger source_length =
        (NSUInteger)source_width * source_height * 4;
    NSUInteger output_length =
        (NSUInteger)output_width * output_height * 4;
    id<MTLBuffer> source = [device newBufferWithBytes:source_rgba
                                               length:source_length
                                              options:MTLResourceStorageModeShared];
    id<MTLBuffer> output = [device newBufferWithLength:output_length
                                               options:MTLResourceStorageModeShared];
    id<MTLBuffer> uniforms = [device newBufferWithBytes:canvas
                                                 length:sizeof(*canvas)
                                                options:MTLResourceStorageModeShared];
    ScreenwideStillOverlay empty_overlay = {0};
    if (overlay == NULL) overlay = &empty_overlay;
    id<MTLBuffer> cursor = cursor_rgba == NULL
        ? [device newBufferWithLength:4 options:MTLResourceStorageModeShared]
        : [device newBufferWithBytes:cursor_rgba
                              length:(NSUInteger)overlay->cursor_source_width *
                                     overlay->cursor_source_height * 4
                             options:MTLResourceStorageModeShared];
    id<MTLBuffer> camera = camera_rgba == NULL
        ? [device newBufferWithLength:4 options:MTLResourceStorageModeShared]
        : [device newBufferWithBytes:camera_rgba
                              length:(NSUInteger)overlay->camera_source_width *
                                     overlay->camera_source_height * 4
                             options:MTLResourceStorageModeShared];
    id<MTLBuffer> overlay_uniforms = [device newBufferWithBytes:overlay
                                                         length:sizeof(*overlay)
                                                        options:MTLResourceStorageModeShared];
    uint32_t source_dimensions[2] = {source_width, source_height};
    float time = (float)seconds;
    id<MTLCommandBuffer> commands = [queue commandBuffer];
    id<MTLComputeCommandEncoder> encoder = [commands computeCommandEncoder];
    [encoder setComputePipelineState:pipeline];
    [encoder setBuffer:source offset:0 atIndex:0];
    [encoder setBuffer:output offset:0 atIndex:1];
    [encoder setBuffer:uniforms offset:0 atIndex:2];
    [encoder setBytes:source_dimensions length:sizeof(source_dimensions) atIndex:3];
    [encoder setBytes:&time length:sizeof(time) atIndex:4];
    [encoder setBuffer:cursor offset:0 atIndex:5];
    [encoder setBuffer:camera offset:0 atIndex:6];
    [encoder setBuffer:overlay_uniforms offset:0 atIndex:7];
    MTLSize grid = MTLSizeMake(output_width, output_height, 1);
    NSUInteger width = MIN(pipeline.threadExecutionWidth, output_width);
    NSUInteger height = MIN(MAX((NSUInteger)1,
      pipeline.maxTotalThreadsPerThreadgroup / MAX(width, (NSUInteger)1)),
      output_height);
    [encoder dispatchThreads:grid threadsPerThreadgroup:MTLSizeMake(width, height, 1)];
    [encoder endEncoding];
    [commands commit];
    [commands waitUntilCompleted];
    if (commands.status == MTLCommandBufferStatusError) {
      return fail(error_text, error_capacity,
                  commands.error.localizedDescription ?:
                    @"The Metal still compositor failed");
    }
    memcpy(output_rgba, output.contents, output_length);
    return 1;
  }
}

@interface ScreenwideStillPresenter : NSObject
@property(nonatomic, strong) id<MTLDevice> device;
@property(nonatomic, strong) id<MTLCommandQueue> queue;
@property(nonatomic, strong) id<MTLComputePipelineState> pipeline;
@property(nonatomic, strong) id<MTLComputePipelineState> unpackPipeline;
@property(nonatomic, strong) id<MTLBuffer> source;
@property(nonatomic, strong) id<MTLBuffer> camera;
@property(nonatomic) CVMetalTextureCacheRef textureCache;
@property(nonatomic) uint64_t sourceToken;
@property(nonatomic) uint32_t sourceWidth;
@property(nonatomic) uint32_t sourceHeight;
@property(nonatomic) uint64_t cameraToken;
@property(nonatomic) uint32_t cameraWidth;
@property(nonatomic) uint32_t cameraHeight;
@end

@implementation ScreenwideStillPresenter
- (void)dealloc {
  if (_textureCache != NULL) CFRelease(_textureCache);
}
@end

void *screenwide_gpu_still_presenter_create(void) {
  @autoreleasepool {
    ScreenwideStillPresenter *presenter = [ScreenwideStillPresenter new];
    presenter.device = MTLCreateSystemDefaultDevice();
    NSError *error = nil;
    id<MTLLibrary> library =
        [presenter.device newLibraryWithSource:shader_source options:nil error:&error];
    presenter.pipeline = [presenter.device newComputePipelineStateWithFunction:
        [library newFunctionWithName:@"present_canvas_rgba"] error:&error];
    presenter.unpackPipeline = [presenter.device newComputePipelineStateWithFunction:
        [library newFunctionWithName:@"unpack_preview_bgra"] error:&error];
    presenter.queue = [presenter.device newCommandQueue];
    CVMetalTextureCacheRef texture_cache = NULL;
    CVMetalTextureCacheCreate(kCFAllocatorDefault, NULL, presenter.device, NULL,
                              &texture_cache);
    presenter.textureCache = texture_cache;
    if (presenter.pipeline == nil || presenter.unpackPipeline == nil ||
        presenter.queue == nil || presenter.textureCache == NULL) return NULL;
    return (__bridge_retained void *)presenter;
  }
}

static id<MTLTexture> preview_texture(ScreenwideStillPresenter *presenter,
                                      CVPixelBufferRef pixels,
                                      CVMetalTextureRef *reference) {
  size_t width = CVPixelBufferGetWidth(pixels);
  size_t height = CVPixelBufferGetHeight(pixels);
  CVReturn result = CVMetalTextureCacheCreateTextureFromImage(
      kCFAllocatorDefault, presenter.textureCache, pixels, NULL,
      MTLPixelFormatBGRA8Unorm, width, height, 0, reference);
  return result == kCVReturnSuccess && *reference != NULL
      ? CVMetalTextureGetTexture(*reference) : nil;
}

static id<MTLBuffer> unpack_pixels(ScreenwideStillPresenter *presenter,
                                   CVPixelBufferRef pixels,
                                   id<MTLCommandBuffer> command,
                                   CVMetalTextureRef *reference) {
  id<MTLTexture> source = preview_texture(presenter, pixels, reference);
  if (source == nil) return nil;
  NSUInteger length = source.width * source.height * 4;
  id<MTLBuffer> output = [presenter.device newBufferWithLength:length
      options:MTLResourceStorageModePrivate];
  id<MTLComputeCommandEncoder> encoder = [command computeCommandEncoder];
  [encoder setComputePipelineState:presenter.unpackPipeline];
  [encoder setTexture:source atIndex:0];
  [encoder setBuffer:output offset:0 atIndex:0];
  MTLSize grid = MTLSizeMake(source.width, source.height, 1);
  NSUInteger width = MIN(presenter.unpackPipeline.threadExecutionWidth, grid.width);
  NSUInteger height = MIN(MAX((NSUInteger)1,
      presenter.unpackPipeline.maxTotalThreadsPerThreadgroup / MAX(width, (NSUInteger)1)),
      grid.height);
  [encoder dispatchThreads:grid threadsPerThreadgroup:MTLSizeMake(width, height, 1)];
  [encoder endEncoding];
  return output;
}

int screenwide_gpu_still_presenter_present_pixels(
    void *handle, void *metal_layer, uint64_t source_token,
    void *source_pixels_handle, const ScreenwideCanvas *canvas, double seconds,
    const uint8_t *cursor_rgba, const uint8_t *camera_rgba,
    void *camera_pixels_handle,
    const ScreenwideStillOverlay *overlay) {
  if (handle == NULL || metal_layer == NULL || source_pixels_handle == NULL ||
      canvas == NULL) return 0;
  @autoreleasepool {
    ScreenwideStillPresenter *presenter = (__bridge ScreenwideStillPresenter *)handle;
    CAMetalLayer *layer = (__bridge CAMetalLayer *)metal_layer;
    CVPixelBufferRef source_pixels = (CVPixelBufferRef)source_pixels_handle;
    uint32_t source_width = (uint32_t)CVPixelBufferGetWidth(source_pixels);
    uint32_t source_height = (uint32_t)CVPixelBufferGetHeight(source_pixels);
    id<CAMetalDrawable> drawable = [layer nextDrawable];
    if (drawable == nil) return 0;
    id<MTLCommandBuffer> command = [presenter.queue commandBuffer];
    CVMetalTextureRef source_reference = NULL;
    if (presenter.source == nil || presenter.sourceToken != source_token ||
        presenter.sourceWidth != source_width || presenter.sourceHeight != source_height) {
      presenter.source = unpack_pixels(presenter, source_pixels, command, &source_reference);
      if (presenter.source == nil) return 0;
      presenter.sourceToken = source_token;
      presenter.sourceWidth = source_width;
      presenter.sourceHeight = source_height;
    }
    ScreenwideStillOverlay empty_overlay = {0};
    if (overlay == NULL) overlay = &empty_overlay;
    id<MTLBuffer> uniforms = [presenter.device newBufferWithBytes:canvas
      length:sizeof(*canvas) options:MTLResourceStorageModeShared];
    id<MTLBuffer> cursor = cursor_rgba == NULL
      ? [presenter.device newBufferWithLength:4 options:MTLResourceStorageModeShared]
      : [presenter.device newBufferWithBytes:cursor_rgba
          length:(NSUInteger)overlay->cursor_source_width * overlay->cursor_source_height * 4
          options:MTLResourceStorageModeShared];
    CVMetalTextureRef camera_reference = NULL;
    if (camera_pixels_handle != NULL) {
      CVPixelBufferRef camera_pixels = (CVPixelBufferRef)camera_pixels_handle;
      uint32_t camera_width = (uint32_t)CVPixelBufferGetWidth(camera_pixels);
      uint32_t camera_height = (uint32_t)CVPixelBufferGetHeight(camera_pixels);
      if (presenter.camera == nil || presenter.cameraToken != source_token ||
          presenter.cameraWidth != camera_width || presenter.cameraHeight != camera_height) {
        presenter.camera = unpack_pixels(presenter, camera_pixels, command,
                                         &camera_reference);
        presenter.cameraToken = source_token;
        presenter.cameraWidth = camera_width;
        presenter.cameraHeight = camera_height;
      }
    } else {
      presenter.camera = nil;
    }
    id<MTLBuffer> camera = presenter.camera != nil
      ? presenter.camera
      : camera_rgba != NULL
        ? [presenter.device newBufferWithBytes:camera_rgba
            length:(NSUInteger)overlay->camera_source_width *
                   overlay->camera_source_height * 4
            options:MTLResourceStorageModeShared]
        : [presenter.device newBufferWithLength:4 options:MTLResourceStorageModeShared];
    if (camera == nil) return 0;
    id<MTLBuffer> overlay_uniforms = [presenter.device newBufferWithBytes:overlay
      length:sizeof(*overlay) options:MTLResourceStorageModeShared];
    uint32_t dimensions[2] = {source_width, source_height};
    float time = (float)seconds;
    id<MTLComputeCommandEncoder> encoder = [command computeCommandEncoder];
    [encoder setComputePipelineState:presenter.pipeline];
    [encoder setBuffer:presenter.source offset:0 atIndex:0];
    [encoder setTexture:drawable.texture atIndex:0];
    [encoder setBuffer:uniforms offset:0 atIndex:1];
    [encoder setBytes:dimensions length:sizeof(dimensions) atIndex:2];
    [encoder setBytes:&time length:sizeof(time) atIndex:3];
    [encoder setBuffer:cursor offset:0 atIndex:4];
    [encoder setBuffer:camera offset:0 atIndex:5];
    [encoder setBuffer:overlay_uniforms offset:0 atIndex:6];
    MTLSize grid = MTLSizeMake(drawable.texture.width, drawable.texture.height, 1);
    NSUInteger width = MIN(presenter.pipeline.threadExecutionWidth, grid.width);
    NSUInteger height = MIN(MAX((NSUInteger)1,
      presenter.pipeline.maxTotalThreadsPerThreadgroup / MAX(width, (NSUInteger)1)), grid.height);
    [encoder dispatchThreads:grid threadsPerThreadgroup:MTLSizeMake(width, height, 1)];
    [encoder endEncoding];
    [command presentDrawable:drawable];
    [command addCompletedHandler:^(__unused id<MTLCommandBuffer> completed) {
      if (source_reference != NULL) CFRelease(source_reference);
      if (camera_reference != NULL) CFRelease(camera_reference);
    }];
    [command commit];
    return 1;
  }
}

int screenwide_gpu_still_presenter_present(
    void *handle, void *metal_layer, uint64_t source_token,
    const uint8_t *source_rgba, uint32_t source_width, uint32_t source_height,
    const ScreenwideCanvas *canvas, double seconds, const uint8_t *cursor_rgba,
    const uint8_t *camera_rgba, const ScreenwideStillOverlay *overlay) {
  if (handle == NULL || metal_layer == NULL || source_rgba == NULL ||
      canvas == NULL || source_width == 0 || source_height == 0) return 0;
  @autoreleasepool {
    ScreenwideStillPresenter *presenter = (__bridge ScreenwideStillPresenter *)handle;
    CAMetalLayer *layer = (__bridge CAMetalLayer *)metal_layer;
    if (presenter.source == nil || presenter.sourceToken != source_token ||
        presenter.sourceWidth != source_width || presenter.sourceHeight != source_height) {
      presenter.source = [presenter.device newBufferWithBytes:source_rgba
          length:(NSUInteger)source_width * source_height * 4
          options:MTLResourceStorageModeShared];
      presenter.sourceToken = source_token;
      presenter.sourceWidth = source_width;
      presenter.sourceHeight = source_height;
    }
    id<CAMetalDrawable> drawable = [layer nextDrawable];
    if (drawable == nil) return 0;
    ScreenwideStillOverlay empty_overlay = {0};
    if (overlay == NULL) overlay = &empty_overlay;
    id<MTLBuffer> uniforms = [presenter.device newBufferWithBytes:canvas
      length:sizeof(*canvas) options:MTLResourceStorageModeShared];
    id<MTLBuffer> cursor = cursor_rgba == NULL
      ? [presenter.device newBufferWithLength:4 options:MTLResourceStorageModeShared]
      : [presenter.device newBufferWithBytes:cursor_rgba
          length:(NSUInteger)overlay->cursor_source_width * overlay->cursor_source_height * 4
          options:MTLResourceStorageModeShared];
    id<MTLBuffer> camera = camera_rgba == NULL
      ? [presenter.device newBufferWithLength:4 options:MTLResourceStorageModeShared]
      : [presenter.device newBufferWithBytes:camera_rgba
          length:(NSUInteger)overlay->camera_source_width * overlay->camera_source_height * 4
          options:MTLResourceStorageModeShared];
    id<MTLBuffer> overlay_uniforms = [presenter.device newBufferWithBytes:overlay
      length:sizeof(*overlay) options:MTLResourceStorageModeShared];
    uint32_t dimensions[2] = {source_width, source_height};
    float time = (float)seconds;
    id<MTLCommandBuffer> command = [presenter.queue commandBuffer];
    id<MTLComputeCommandEncoder> encoder = [command computeCommandEncoder];
    [encoder setComputePipelineState:presenter.pipeline];
    [encoder setBuffer:presenter.source offset:0 atIndex:0];
    [encoder setTexture:drawable.texture atIndex:0];
    [encoder setBuffer:uniforms offset:0 atIndex:1];
    [encoder setBytes:dimensions length:sizeof(dimensions) atIndex:2];
    [encoder setBytes:&time length:sizeof(time) atIndex:3];
    [encoder setBuffer:cursor offset:0 atIndex:4];
    [encoder setBuffer:camera offset:0 atIndex:5];
    [encoder setBuffer:overlay_uniforms offset:0 atIndex:6];
    MTLSize grid = MTLSizeMake(drawable.texture.width, drawable.texture.height, 1);
    NSUInteger width = MIN(presenter.pipeline.threadExecutionWidth, grid.width);
    NSUInteger height = MIN(MAX((NSUInteger)1,
      presenter.pipeline.maxTotalThreadsPerThreadgroup / MAX(width, (NSUInteger)1)), grid.height);
    [encoder dispatchThreads:grid threadsPerThreadgroup:MTLSizeMake(width, height, 1)];
    [encoder endEncoding];
    [command presentDrawable:drawable];
    [command commit];
    return 1;
  }
}

void screenwide_gpu_still_presenter_destroy(void *handle) {
  if (handle != NULL) CFBridgingRelease(handle);
}
