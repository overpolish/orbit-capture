// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#import <AVFoundation/AVFoundation.h>
#import <Metal/Metal.h>
#import <QuartzCore/CAMetalLayer.h>
#import <VideoToolbox/VideoToolbox.h>
#include <math.h>

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
  int32_t frame_x;
  int32_t frame_y;
  uint32_t frame_width;
  uint32_t frame_height;
  uint32_t radius;
  uint32_t drop_shadow;
  uint32_t camera_on_top;
} ScreenwideCameraOverlay;

typedef struct {
  uint32_t crop_x;
  uint32_t crop_y;
  uint32_t crop_width;
  uint32_t crop_height;
  int32_t frame_x;
  int32_t frame_y;
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
    id<MTLComputePipelineState> screen_luma_pipeline =
        [device newComputePipelineStateWithFunction:
                    [library newFunctionWithName:@"overlay_screen_luma"]
                                              error:&error];
    id<MTLComputePipelineState> screen_chroma_pipeline =
        [device newComputePipelineStateWithFunction:
                    [library newFunctionWithName:@"overlay_screen_chroma"]
                                              error:&error];
    id<MTLCommandQueue> queue = [device newCommandQueue];
    CVMetalTextureCacheRef texture_cache = NULL;
    CVMetalTextureCacheCreate(kCFAllocatorDefault, NULL, device, NULL,
                              &texture_cache);
    if (device == nil || library == nil || luma_pipeline == nil ||
        chroma_pipeline == nil || camera_luma_pipeline == nil ||
        camera_chroma_pipeline == nil || queue == nil ||
        canvas_luma_pipeline == nil || canvas_chroma_pipeline == nil ||
        screen_luma_pipeline == nil || screen_chroma_pipeline == nil ||
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
        CVMetalTextureRef cursor_top_ref = NULL;
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
        if (camera_sample != NULL && camera_overlay != NULL &&
            camera_overlay->camera_on_top == 0) {
          MTLSize screen_group = MTLSizeMake(16, 16, 1);
          id<MTLComputeCommandEncoder> screen_compute =
              [command computeCommandEncoder];
          [screen_compute setComputePipelineState:screen_luma_pipeline];
          [screen_compute setTexture:source_y atIndex:0];
          [screen_compute setTexture:source_uv atIndex:1];
          [screen_compute setTexture:destination_y atIndex:2];
          [screen_compute setBytes:canvas length:sizeof(*canvas) atIndex:0];
          [screen_compute dispatchThreads:MTLSizeMake(y_width, y_height, 1)
                       threadsPerThreadgroup:screen_group];
          [screen_compute endEncoding];
          screen_compute = [command computeCommandEncoder];
          [screen_compute setComputePipelineState:screen_chroma_pipeline];
          [screen_compute setTexture:source_y atIndex:0];
          [screen_compute setTexture:source_uv atIndex:1];
          [screen_compute setTexture:destination_uv atIndex:2];
          [screen_compute setBytes:canvas length:sizeof(*canvas) atIndex:0];
          [screen_compute dispatchThreads:MTLSizeMake(uv_width, uv_height, 1)
                       threadsPerThreadgroup:screen_group];
          [screen_compute endEncoding];

          // Cursor belongs to the screen layer. Reapply it after the screen
          // when the camera has been sent behind that layer.
          if (cursor_sample != NULL) {
            CVPixelBufferRef cursor_pixels =
                CMSampleBufferGetImageBuffer(cursor_sample);
            size_t cursor_width = CVPixelBufferGetWidth(cursor_pixels);
            size_t cursor_height = CVPixelBufferGetHeight(cursor_pixels);
            id<MTLTexture> cursor_texture =
                texture(texture_cache, cursor_pixels, MTLPixelFormatBGRA8Unorm,
                        cursor_width, cursor_height, 0, &cursor_top_ref);
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
            id<MTLComputeCommandEncoder> cursor_compute =
                [command computeCommandEncoder];
            [cursor_compute setComputePipelineState:luma_pipeline];
            [cursor_compute setTexture:cursor_texture atIndex:0];
            [cursor_compute setTexture:destination_y atIndex:1];
            [cursor_compute setBytes:&uniforms length:sizeof(uniforms) atIndex:0];
            [cursor_compute dispatchThreads:MTLSizeMake(cursor_width, cursor_height, 1)
                         threadsPerThreadgroup:screen_group];
            [cursor_compute endEncoding];
            cursor_compute = [command computeCommandEncoder];
            [cursor_compute setComputePipelineState:chroma_pipeline];
            [cursor_compute setTexture:cursor_texture atIndex:0];
            [cursor_compute setTexture:destination_uv atIndex:1];
            [cursor_compute setBytes:&uniforms length:sizeof(uniforms) atIndex:0];
            [cursor_compute dispatchThreads:MTLSizeMake((cursor_width + 1) / 2,
                                                        (cursor_height + 1) / 2, 1)
                         threadsPerThreadgroup:screen_group];
            [cursor_compute endEncoding];
          }
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
        if (cursor_top_ref != NULL)
          CFRelease(cursor_top_ref);
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

int screenwide_gpu_alpha_composite(const uint8_t *base_rgba,
                                   const uint8_t *overlay_rgba,
                                   uint32_t width,
                                   uint32_t height,
                                   uint8_t *output_rgba,
                                   char *error_text,
                                   size_t error_capacity) {
  @autoreleasepool {
    if (base_rgba == NULL || overlay_rgba == NULL || output_rgba == NULL ||
        width == 0 || height == 0) {
      return fail(error_text, error_capacity,
                  @"The GPU layer compositor received invalid pixels");
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
          [library newFunctionWithName:@"alpha_composite_rgba"];
      pipeline = [device newComputePipelineStateWithFunction:function error:&error];
      queue = [device newCommandQueue];
      initialization_error = error.localizedDescription;
    });
    if (device == nil || pipeline == nil || queue == nil) {
      return fail(error_text, error_capacity,
                  initialization_error ?:
                    @"The Metal layer compositor could not be created");
    }
    NSUInteger pixel_count = (NSUInteger)width * height;
    NSUInteger byte_length = pixel_count * 4;
    id<MTLBuffer> base = [device newBufferWithBytes:base_rgba
                                             length:byte_length
                                            options:MTLResourceStorageModeShared];
    id<MTLBuffer> overlay = [device newBufferWithBytes:overlay_rgba
                                                length:byte_length
                                               options:MTLResourceStorageModeShared];
    id<MTLBuffer> output = [device newBufferWithLength:byte_length
                                               options:MTLResourceStorageModeShared];
    id<MTLCommandBuffer> commands = [queue commandBuffer];
    id<MTLComputeCommandEncoder> encoder = [commands computeCommandEncoder];
    [encoder setComputePipelineState:pipeline];
    [encoder setBuffer:base offset:0 atIndex:0];
    [encoder setBuffer:overlay offset:0 atIndex:1];
    [encoder setBuffer:output offset:0 atIndex:2];
    NSUInteger group_width = MIN(pipeline.threadExecutionWidth, pixel_count);
    [encoder dispatchThreads:MTLSizeMake(pixel_count, 1, 1)
        threadsPerThreadgroup:MTLSizeMake(MAX(group_width, (NSUInteger)1), 1, 1)];
    [encoder endEncoding];
    [commands commit];
    [commands waitUntilCompleted];
    if (commands.status == MTLCommandBufferStatusError) {
      return fail(error_text, error_capacity,
                  commands.error.localizedDescription ?:
                    @"The Metal layer compositor failed");
    }
    memcpy(output_rgba, output.contents, byte_length);
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
@property(nonatomic, strong) NSMutableDictionary<NSNumber *, id<MTLBuffer>> *workspaceSources;
@property(nonatomic, strong) NSMutableDictionary<NSNumber *, id<MTLBuffer>> *workspaceCursorSources;
@property(nonatomic, strong) NSMutableDictionary<NSNumber *, id<MTLBuffer>> *workspaceCameraSources;
@property(nonatomic, strong) NSMutableDictionary<NSNumber *, NSValue *> *workspaceSourceSizes;
@property(nonatomic, strong) NSMutableArray<NSValue *> *workspaceLayers;
@property(nonatomic, strong) NSArray<NSValue *> *workspaceResizeLayers;
@property(nonatomic, strong) id<MTLComputePipelineState> workspaceClearPipeline;
@property(nonatomic, strong) id<MTLComputePipelineState> workspaceLayerPipeline;
@property(nonatomic, strong) id<MTLComputePipelineState> workspaceMagnifierPipeline;
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
    presenter.workspaceClearPipeline = [presenter.device newComputePipelineStateWithFunction:
        [library newFunctionWithName:@"workspace_clear"] error:&error];
    presenter.workspaceLayerPipeline = [presenter.device newComputePipelineStateWithFunction:
        [library newFunctionWithName:@"workspace_layer"] error:&error];
    presenter.workspaceMagnifierPipeline = [presenter.device newComputePipelineStateWithFunction:
        [library newFunctionWithName:@"workspace_magnifier"] error:&error];
    presenter.queue = [presenter.device newCommandQueue];
    presenter.workspaceSources = [NSMutableDictionary dictionary];
    presenter.workspaceCursorSources = [NSMutableDictionary dictionary];
    presenter.workspaceCameraSources = [NSMutableDictionary dictionary];
    presenter.workspaceSourceSizes = [NSMutableDictionary dictionary];
    presenter.workspaceLayers = [NSMutableArray array];
    CVMetalTextureCacheRef texture_cache = NULL;
    CVMetalTextureCacheCreate(kCFAllocatorDefault, NULL, presenter.device, NULL,
                              &texture_cache);
    presenter.textureCache = texture_cache;
    if (presenter.pipeline == nil || presenter.unpackPipeline == nil ||
        presenter.workspaceClearPipeline == nil || presenter.workspaceLayerPipeline == nil ||
        presenter.workspaceMagnifierPipeline == nil ||
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
    const ScreenwideStillOverlay *overlay,
    ScreenwidePresentBlock present) {
  if (handle == NULL || metal_layer == NULL || source_pixels_handle == NULL ||
      canvas == NULL || present == NULL) return 0;
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
    [command addCompletedHandler:^(__unused id<MTLCommandBuffer> completed) {
      if (source_reference != NULL) CFRelease(source_reference);
      if (camera_reference != NULL) CFRelease(camera_reference);
    }];
    present((__bridge void *)command, (__bridge void *)drawable);
    return 1;
  }
}

static id<MTLBuffer> workspace_source_buffer(
    ScreenwideStillPresenter *presenter, const ScreenwideWorkspaceLayer *layer) {
  if (layer->source_width == 0 || layer->source_height == 0) return nil;
  NSNumber *key = @(layer->source_token);
  NSValue *known_size = presenter.workspaceSourceSizes[key];
  uint32_t dimensions[2] = {layer->source_width, layer->source_height};
  if (known_size != nil) {
    uint32_t cached[2] = {0, 0};
    [known_size getValue:cached size:sizeof(cached)];
    if (cached[0] == dimensions[0] && cached[1] == dimensions[1])
      return presenter.workspaceSources[key];
  }
  if (layer->source_rgba == NULL && layer->source_pixels == NULL) return nil;
  NSUInteger length = (NSUInteger)layer->source_width * layer->source_height * 4;
  if (layer->source_kind != 0 && layer->source_pixels != NULL) return nil;
  if (layer->source_rgba == NULL) return nil;
  id<MTLBuffer> buffer = [presenter.device newBufferWithBytes:layer->source_rgba
      length:length options:MTLResourceStorageModeShared];
  if (buffer == nil) return nil;
  presenter.workspaceSources[key] = buffer;
  presenter.workspaceSourceSizes[key] = [NSValue valueWithBytes:dimensions
                                                         objCType:@encode(uint32_t[2])];
  return buffer;
}

static void workspace_dispatch(
    id<MTLComputeCommandEncoder> encoder, id<MTLComputePipelineState> pipeline,
    MTLSize grid) {
  NSUInteger width = MIN(pipeline.threadExecutionWidth, grid.width);
  NSUInteger height = MIN(MAX((NSUInteger)1,
      pipeline.maxTotalThreadsPerThreadgroup / MAX(width, (NSUInteger)1)),
      grid.height);
  [encoder dispatchThreads:grid threadsPerThreadgroup:MTLSizeMake(
      MAX(width, (NSUInteger)1), MAX(height, (NSUInteger)1), 1)];
}

static int presenter_present_workspace_layers(
    ScreenwideStillPresenter *presenter, CAMetalLayer *layer,
    const ScreenwideWorkspaceLayer *layers, uint32_t layer_count,
    ScreenwidePresentBlock present) {
  if (layer_count == 0 || layers == NULL || present == NULL) return 0;
  id<CAMetalDrawable> drawable = [layer nextDrawable];
  if (drawable == nil) return 0;
  id<MTLCommandBuffer> command = [presenter.queue commandBuffer];
  if (command == nil) return 0;
  MTLSize grid = MTLSizeMake(drawable.texture.width, drawable.texture.height, 1);
  id<MTLComputeCommandEncoder> clear = [command computeCommandEncoder];
  [clear setComputePipelineState:presenter.workspaceClearPipeline];
  [clear setTexture:drawable.texture atIndex:0];
  workspace_dispatch(clear, presenter.workspaceClearPipeline, grid);
  [clear endEncoding];
  NSMutableArray<NSValue *> *pixelReferences = [NSMutableArray array];
  for (uint32_t index = 0; index < layer_count; ++index) {
    const ScreenwideWorkspaceLayer *item = &layers[index];
    id<MTLBuffer> source = workspace_source_buffer(presenter, item);
    CVMetalTextureRef source_reference = NULL;
    if (source == nil && item->source_kind != 0 && item->source_pixels != NULL) {
      source = unpack_pixels(presenter, (CVPixelBufferRef)item->source_pixels,
                             command, &source_reference);
      if (source != nil) {
        presenter.workspaceSources[@(item->source_token)] = source;
        uint32_t dimensions[2] = {item->source_width, item->source_height};
        presenter.workspaceSourceSizes[@(item->source_token)] =
            [NSValue valueWithBytes:dimensions objCType:@encode(uint32_t[2])];
        if (source_reference != NULL)
          [pixelReferences addObject:[NSValue valueWithPointer:source_reference]];
      }
    }
    if (source == nil) return 0;
    if (item->placement.width == 0 || item->placement.height == 0) continue;
    id<MTLBuffer> uniforms = [presenter.device newBufferWithBytes:&item->canvas
        length:sizeof(item->canvas) options:MTLResourceStorageModeShared];
    if (uniforms == nil) return 0;
    NSUInteger cursor_length = (NSUInteger)item->overlay.cursor_source_width *
        item->overlay.cursor_source_height * 4;
    NSUInteger camera_length = (NSUInteger)item->overlay.camera_source_width *
        item->overlay.camera_source_height * 4;
    NSNumber *token = @(item->source_token);
    id<MTLBuffer> cursor = presenter.workspaceCursorSources[token];
    if (cursor == nil)
      cursor = item->cursor_rgba != NULL && cursor_length > 0
          ? [presenter.device newBufferWithBytes:item->cursor_rgba length:cursor_length
            options:MTLResourceStorageModeShared]
          : [presenter.device newBufferWithLength:4 options:MTLResourceStorageModeShared];
    id<MTLBuffer> camera = presenter.workspaceCameraSources[token];
    if (camera == nil)
      camera = item->camera_rgba != NULL && camera_length > 0
          ? [presenter.device newBufferWithBytes:item->camera_rgba length:camera_length
            options:MTLResourceStorageModeShared]
          : [presenter.device newBufferWithLength:4 options:MTLResourceStorageModeShared];
    CVMetalTextureRef camera_reference = NULL;
    if (presenter.workspaceCameraSources[token] == nil &&
        item->camera_rgba == NULL && item->camera_pixels != NULL) {
      camera = unpack_pixels(presenter, (CVPixelBufferRef)item->camera_pixels,
                             command, &camera_reference);
      if (camera == nil) return 0;
      if (camera_reference != NULL)
        [pixelReferences addObject:[NSValue valueWithPointer:camera_reference]];
    }
    id<MTLBuffer> overlay = [presenter.device newBufferWithBytes:&item->overlay
        length:sizeof(item->overlay) options:MTLResourceStorageModeShared];
    if (cursor == nil || camera == nil || overlay == nil) return 0;
    presenter.workspaceCursorSources[token] = cursor;
    presenter.workspaceCameraSources[token] = camera;
    uint32_t dimensions[2] = {item->source_width, item->source_height};
    uint32_t first = index == 0 ? 1 : 0;
    id<MTLComputeCommandEncoder> encoder = [command computeCommandEncoder];
    [encoder setComputePipelineState:presenter.workspaceLayerPipeline];
    [encoder setBuffer:source offset:0 atIndex:0];
    [encoder setTexture:drawable.texture atIndex:0];
    [encoder setBuffer:uniforms offset:0 atIndex:1];
    [encoder setBytes:dimensions length:sizeof(dimensions) atIndex:2];
    [encoder setBytes:&item->placement length:sizeof(item->placement) atIndex:3];
    [encoder setBytes:&first length:sizeof(first) atIndex:4];
    uint32_t logical[2] = {item->canvas_width, item->canvas_height};
    [encoder setBytes:logical length:sizeof(logical) atIndex:5];
    [encoder setBuffer:cursor offset:0 atIndex:6];
    [encoder setBuffer:camera offset:0 atIndex:7];
    [encoder setBuffer:overlay offset:0 atIndex:8];
    [encoder setBytes:&item->seconds length:sizeof(item->seconds) atIndex:9];
    workspace_dispatch(encoder, presenter.workspaceLayerPipeline, grid);
    [encoder endEncoding];
  }
  [command addCompletedHandler:^(__unused id<MTLCommandBuffer> completed) {
    for (NSValue *value in pixelReferences) {
      CVMetalTextureRef reference = [value pointerValue];
      if (reference != NULL) CFRelease(reference);
    }
  }];
  present((__bridge void *)command, (__bridge void *)drawable);
  return 1;
}

int screenwide_gpu_still_presenter_set_workspace(
    void *handle, const ScreenwideWorkspaceLayer *layers,
    uint32_t layer_count) {
  if (handle == NULL || layers == NULL || layer_count == 0) return 0;
  @autoreleasepool {
    ScreenwideStillPresenter *presenter = (__bridge ScreenwideStillPresenter *)handle;
    NSMutableArray<NSValue *> *retained = [NSMutableArray arrayWithCapacity:layer_count];
    NSMutableSet<NSNumber *> *activeTokens = [NSMutableSet setWithCapacity:layer_count];
    id<MTLCommandBuffer> uploadCommand = nil;
    NSMutableArray<NSValue *> *pixelReferences = [NSMutableArray array];
    for (uint32_t index = 0; index < layer_count; ++index) {
      id<MTLBuffer> source = workspace_source_buffer(presenter, &layers[index]);
      if (source == nil && layers[index].source_kind != 0 &&
          layers[index].source_pixels != NULL) {
        if (uploadCommand == nil) uploadCommand = [presenter.queue commandBuffer];
        CVMetalTextureRef reference = NULL;
        source = unpack_pixels(
            presenter, (CVPixelBufferRef)layers[index].source_pixels,
            uploadCommand, &reference);
        if (source != nil) {
          presenter.workspaceSources[@(layers[index].source_token)] = source;
          uint32_t dimensions[2] = {layers[index].source_width,
                                    layers[index].source_height};
          presenter.workspaceSourceSizes[@(layers[index].source_token)] =
              [NSValue valueWithBytes:dimensions
                              objCType:@encode(uint32_t[2])];
          if (reference != NULL)
            [pixelReferences addObject:[NSValue valueWithPointer:reference]];
        }
      }
      if (source == nil) return 0;
      NSNumber *token = @(layers[index].source_token);
      NSUInteger cursorLength = (NSUInteger)layers[index].overlay.cursor_source_width *
          layers[index].overlay.cursor_source_height * 4;
      if (layers[index].cursor_rgba != NULL && cursorLength > 0)
        presenter.workspaceCursorSources[token] =
            [presenter.device newBufferWithBytes:layers[index].cursor_rgba
                                          length:cursorLength
                                         options:MTLResourceStorageModeShared];
      else
        [presenter.workspaceCursorSources removeObjectForKey:token];
      NSUInteger cameraLength = (NSUInteger)layers[index].overlay.camera_source_width *
          layers[index].overlay.camera_source_height * 4;
      if (layers[index].camera_rgba != NULL && cameraLength > 0)
        presenter.workspaceCameraSources[token] =
            [presenter.device newBufferWithBytes:layers[index].camera_rgba
                                          length:cameraLength
                                         options:MTLResourceStorageModeShared];
      else if (layers[index].camera_pixels != NULL) {
        if (uploadCommand == nil) uploadCommand = [presenter.queue commandBuffer];
        CVMetalTextureRef reference = NULL;
        id<MTLBuffer> camera = unpack_pixels(
            presenter, (CVPixelBufferRef)layers[index].camera_pixels,
            uploadCommand, &reference);
        if (camera == nil) return 0;
        presenter.workspaceCameraSources[token] = camera;
        if (reference != NULL)
          [pixelReferences addObject:[NSValue valueWithPointer:reference]];
      } else
        [presenter.workspaceCameraSources removeObjectForKey:token];
      [retained addObject:[NSValue valueWithBytes:&layers[index]
                                          objCType:@encode(ScreenwideWorkspaceLayer)]];
      [activeTokens addObject:token];
    }
    if (uploadCommand != nil) {
      [uploadCommand addCompletedHandler:^(__unused id<MTLCommandBuffer> completed) {
        for (NSValue *value in pixelReferences) {
          CVMetalTextureRef reference = [value pointerValue];
          if (reference != NULL) CFRelease(reference);
        }
      }];
      [uploadCommand commit];
    }
    for (NSNumber *key in [presenter.workspaceSources.allKeys copy])
      if (![activeTokens containsObject:key]) {
        [presenter.workspaceSources removeObjectForKey:key];
        [presenter.workspaceSourceSizes removeObjectForKey:key];
        [presenter.workspaceCursorSources removeObjectForKey:key];
        [presenter.workspaceCameraSources removeObjectForKey:key];
      }
    presenter.workspaceLayers = retained;
    return 1;
  }
}

int screenwide_gpu_still_presenter_workspace_source_size(
    void *handle, uint32_t pane_index, uint32_t *width, uint32_t *height) {
  if (handle == NULL || width == NULL || height == NULL) return 0;
  ScreenwideStillPresenter *presenter = (__bridge ScreenwideStillPresenter *)handle;
  for (NSValue *value in presenter.workspaceLayers) {
    ScreenwideWorkspaceLayer layer;
    [value getValue:&layer size:sizeof(layer)];
    if (layer.pane_index != pane_index) continue;
    if (presenter.workspaceSources[@(layer.source_token)] == nil) return 0;
    *width = layer.source_width;
    *height = layer.source_height;
    return layer.source_width > 0 && layer.source_height > 0;
  }
  return 0;
}

int screenwide_gpu_still_presenter_workspace_canvas_size(
    void *handle, uint32_t pane_index, uint32_t *width, uint32_t *height) {
  if (handle == NULL || width == NULL || height == NULL) return 0;
  ScreenwideStillPresenter *presenter = (__bridge ScreenwideStillPresenter *)handle;
  for (NSValue *value in presenter.workspaceLayers) {
    ScreenwideWorkspaceLayer layer;
    [value getValue:&layer size:sizeof(layer)];
    if (layer.pane_index != pane_index) continue;
    *width = layer.canvas_width;
    *height = layer.canvas_height;
    return *width > 0 && *height > 0;
  }
  return 0;
}

int screenwide_gpu_still_presenter_workspace_camera_source_size(
    void *handle, uint32_t pane_index, uint32_t *width, uint32_t *height) {
  if (handle == NULL || width == NULL || height == NULL) return 0;
  ScreenwideStillPresenter *presenter = (__bridge ScreenwideStillPresenter *)handle;
  for (NSValue *value in presenter.workspaceLayers) {
    ScreenwideWorkspaceLayer layer;
    [value getValue:&layer size:sizeof(layer)];
    if (layer.pane_index != pane_index) continue;
    if (presenter.workspaceCameraSources[@(layer.source_token)] == nil) return 0;
    *width = layer.overlay.camera_source_width;
    *height = layer.overlay.camera_source_height;
    return *width > 0 && *height > 0;
  }
  return 0;
}

int screenwide_gpu_still_presenter_update_workspace_canvas(
    void *handle, uint32_t pane_index, uint32_t canvas_width,
    uint32_t canvas_height, const ScreenwideCanvas *canvas) {
  if (handle == NULL || canvas == NULL || canvas_width == 0 || canvas_height == 0)
    return 0;
  ScreenwideStillPresenter *presenter = (__bridge ScreenwideStillPresenter *)handle;
  NSMutableArray<NSValue *> *updated = [presenter.workspaceLayers mutableCopy];
  for (NSUInteger index = 0; index < updated.count; ++index) {
    ScreenwideWorkspaceLayer layer;
    [updated[index] getValue:&layer size:sizeof(layer)];
    if (layer.pane_index != pane_index) continue;
    if (presenter.workspaceSources[@(layer.source_token)] == nil) return 0;
    layer.canvas_width = canvas_width;
    layer.canvas_height = canvas_height;
    layer.canvas = *canvas;
    updated[index] = [NSValue valueWithBytes:&layer
                                     objCType:@encode(ScreenwideWorkspaceLayer)];
    presenter.workspaceLayers = updated;
    return 1;
  }
  return 0;
}

int screenwide_gpu_still_presenter_update_workspace_camera_overlay(
    void *handle, uint32_t pane_index, const ScreenwideStillOverlay *overlay) {
  if (handle == NULL || overlay == NULL) return 0;
  ScreenwideStillPresenter *presenter = (__bridge ScreenwideStillPresenter *)handle;
  NSMutableArray<NSValue *> *updated = [presenter.workspaceLayers mutableCopy];
  for (NSUInteger index = 0; index < updated.count; ++index) {
    ScreenwideWorkspaceLayer layer;
    [updated[index] getValue:&layer size:sizeof(layer)];
    if (layer.pane_index != pane_index) continue;
    if (presenter.workspaceCameraSources[@(layer.source_token)] == nil) return 0;
    layer.overlay.camera_crop_x = overlay->camera_crop_x;
    layer.overlay.camera_crop_y = overlay->camera_crop_y;
    layer.overlay.camera_crop_width = overlay->camera_crop_width;
    layer.overlay.camera_crop_height = overlay->camera_crop_height;
    layer.overlay.camera_frame_x = overlay->camera_frame_x;
    layer.overlay.camera_frame_y = overlay->camera_frame_y;
    layer.overlay.camera_frame_width = overlay->camera_frame_width;
    layer.overlay.camera_frame_height = overlay->camera_frame_height;
    layer.overlay.camera_radius = overlay->camera_radius;
    layer.overlay.camera_source_width = overlay->camera_source_width;
    layer.overlay.camera_source_height = overlay->camera_source_height;
    layer.overlay.camera_drop_shadow = overlay->camera_drop_shadow;
    layer.overlay.camera_on_top = overlay->camera_on_top;
    updated[index] = [NSValue valueWithBytes:&layer
                                     objCType:@encode(ScreenwideWorkspaceLayer)];
    presenter.workspaceLayers = updated;
    return 1;
  }
  return 0;
}

int screenwide_gpu_still_presenter_begin_workspace_resize(void *handle) {
  if (handle == NULL) return 0;
  ScreenwideStillPresenter *presenter = (__bridge ScreenwideStillPresenter *)handle;
  presenter.workspaceResizeLayers = [presenter.workspaceLayers copy];
  return presenter.workspaceResizeLayers.count > 0;
}

static int update_workspace_resize(
    void *handle, uint32_t selected_layer, double move_x_ratio,
    double move_y_ratio, double origin_x_ratio, double origin_y_ratio,
    double width_ratio, double height_ratio) {
  if (handle == NULL || !isfinite(origin_x_ratio) ||
      !isfinite(origin_y_ratio) || !isfinite(width_ratio) ||
      !isfinite(height_ratio) || width_ratio <= 0.0 ||
      height_ratio <= 0.0) return 0;
  ScreenwideStillPresenter *presenter = (__bridge ScreenwideStillPresenter *)handle;
  if (presenter.workspaceResizeLayers.count == 0) return 0;
  NSMutableArray<NSValue *> *resized =
      [NSMutableArray arrayWithCapacity:presenter.workspaceResizeLayers.count];
  NSUInteger index = 0;
  for (NSValue *value in presenter.workspaceResizeLayers) {
    ScreenwideWorkspaceLayer layer;
    [value getValue:&layer size:sizeof(layer)];
    double old_width = MAX(layer.canvas_width, 1u);
    double old_height = MAX(layer.canvas_height, 1u);
    uint32_t next_width = (uint32_t)MAX(llround(old_width * width_ratio), 1);
    uint32_t next_height = (uint32_t)MAX(llround(old_height * height_ratio), 1);
    double origin_x = old_width * origin_x_ratio;
    double origin_y = old_height * origin_y_ratio;
    double move_x = index == selected_layer ? old_width * move_x_ratio : 0.0;
    double move_y = index == selected_layer ? old_height * move_y_ratio : 0.0;
    double old_shortest = MAX(MIN(old_width, old_height), 1.0);
    double next_shortest = MIN(next_width, next_height);
    layer.canvas_width = next_width;
    layer.canvas_height = next_height;
    layer.canvas.crop_x =
        (int32_t)llround(layer.canvas.crop_x + move_x - origin_x);
    layer.canvas.crop_y =
        (int32_t)llround(layer.canvas.crop_y + move_y - origin_y);
    layer.canvas.image_x += (float)(move_x - origin_x);
    layer.canvas.image_y += (float)(move_y - origin_y);
    if (layer.overlay.cursor_width > 0) {
      layer.overlay.cursor_x += (int32_t)llround(move_x - origin_x);
      layer.overlay.cursor_y += (int32_t)llround(move_y - origin_y);
    }
    layer.canvas.background_radius = (uint32_t)MAX(
        llround(layer.canvas.background_radius * next_shortest / old_shortest), 0);
    [resized addObject:[NSValue valueWithBytes:&layer
                                       objCType:@encode(ScreenwideWorkspaceLayer)]];
    index += 1;
  }
  presenter.workspaceLayers = resized;
  return 1;
}

int screenwide_gpu_still_presenter_update_workspace_resize(
    void *handle, double origin_x_ratio, double origin_y_ratio,
    double width_ratio, double height_ratio) {
  return update_workspace_resize(
      handle, UINT32_MAX, 0.0, 0.0, origin_x_ratio, origin_y_ratio,
      width_ratio, height_ratio);
}

int screenwide_gpu_still_presenter_update_workspace_auto_fit_move(
    void *handle, uint32_t selected_layer, double move_x_ratio,
    double move_y_ratio, double origin_x_ratio, double origin_y_ratio,
    double width_ratio, double height_ratio) {
  return update_workspace_resize(
      handle, selected_layer, move_x_ratio, move_y_ratio,
      origin_x_ratio, origin_y_ratio, width_ratio, height_ratio);
}

int screenwide_gpu_still_presenter_update_recording_auto_fit_move(
    void *handle, uint32_t selected_pane, double move_x_ratio,
    double move_y_ratio, double origin_x_ratio, double origin_y_ratio,
    double width_ratio, double height_ratio) {
  if (handle == NULL || !isfinite(origin_x_ratio) ||
      !isfinite(origin_y_ratio) || !isfinite(width_ratio) ||
      !isfinite(height_ratio) || width_ratio <= 0.0 || height_ratio <= 0.0)
    return 0;
  ScreenwideStillPresenter *presenter = (__bridge ScreenwideStillPresenter *)handle;
  if (presenter.workspaceResizeLayers.count == 0) return 0;
  BOOL bakedCamera = selected_pane == 1 && presenter.workspaceResizeLayers.count == 1;
  NSMutableArray<NSValue *> *resized =
      [NSMutableArray arrayWithCapacity:presenter.workspaceResizeLayers.count];
  BOOL found = NO;
  for (NSValue *value in presenter.workspaceResizeLayers) {
    ScreenwideWorkspaceLayer layer;
    [value getValue:&layer size:sizeof(layer)];
    if (layer.pane_index == selected_pane || bakedCamera) {
      found = YES;
      double oldWidth = MAX(layer.canvas_width, 1u);
      double oldHeight = MAX(layer.canvas_height, 1u);
      double originX = oldWidth * origin_x_ratio;
      double originY = oldHeight * origin_y_ratio;
      double moveX = oldWidth * move_x_ratio;
      double moveY = oldHeight * move_y_ratio;
      layer.canvas_width = (uint32_t)MAX(llround(oldWidth * width_ratio), 1);
      layer.canvas_height = (uint32_t)MAX(llround(oldHeight * height_ratio), 1);
      layer.canvas.crop_x -= (int32_t)llround(originX);
      layer.canvas.crop_y -= (int32_t)llround(originY);
      layer.canvas.image_x -= (float)originX;
      layer.canvas.image_y -= (float)originY;
      if (bakedCamera) {
        layer.overlay.camera_frame_x += (int32_t)llround(moveX - originX);
        layer.overlay.camera_frame_y += (int32_t)llround(moveY - originY);
        // The selected camera moves independently, while the cursor remains
        // attached to the screen content whose canvas origin just changed.
        if (layer.overlay.cursor_width > 0) {
          layer.overlay.cursor_x -= (int32_t)llround(originX);
          layer.overlay.cursor_y -= (int32_t)llround(originY);
        }
      } else {
        layer.canvas.crop_x += (int32_t)llround(moveX);
        layer.canvas.crop_y += (int32_t)llround(moveY);
        layer.canvas.image_x += (float)moveX;
        layer.canvas.image_y += (float)moveY;
        if (layer.overlay.cursor_width > 0) {
          layer.overlay.cursor_x += (int32_t)llround(moveX - originX);
          layer.overlay.cursor_y += (int32_t)llround(moveY - originY);
        }
      }
      layer.placement.width =
          (uint32_t)MAX(llround(layer.placement.width * width_ratio), 1);
      layer.placement.height =
          (uint32_t)MAX(llround(layer.placement.height * height_ratio), 1);
    }
    [resized addObject:[NSValue valueWithBytes:&layer
                                       objCType:@encode(ScreenwideWorkspaceLayer)]];
  }
  if (!found) return 0;
  presenter.workspaceLayers = resized;
  return 1;
}

int screenwide_gpu_still_presenter_update_workspace_selected_resize(
    void *handle, uint32_t selected_layer, double origin_x_ratio,
    double origin_y_ratio, double width_ratio, double height_ratio) {
  if (handle == NULL || !isfinite(origin_x_ratio) ||
      !isfinite(origin_y_ratio) || !isfinite(width_ratio) ||
      !isfinite(height_ratio) || width_ratio <= 0.0 || height_ratio <= 0.0)
    return 0;
  ScreenwideStillPresenter *presenter = (__bridge ScreenwideStillPresenter *)handle;
  NSArray<NSValue *> *base = presenter.workspaceResizeLayers.count > 0
      ? presenter.workspaceResizeLayers
      : presenter.workspaceLayers;
  NSMutableArray<NSValue *> *resized =
      [NSMutableArray arrayWithCapacity:base.count];
  BOOL found = NO;
  for (NSValue *value in base) {
    ScreenwideWorkspaceLayer layer;
    [value getValue:&layer size:sizeof(layer)];
    if (layer.pane_index == selected_layer) {
      found = YES;
      double old_width = MAX(layer.canvas_width, 1u);
      double old_height = MAX(layer.canvas_height, 1u);
      uint32_t next_width = (uint32_t)MAX(llround(old_width * width_ratio), 1);
      uint32_t next_height = (uint32_t)MAX(llround(old_height * height_ratio), 1);
      double old_shortest = MAX(MIN(old_width, old_height), 1.0);
      double next_shortest = MIN(next_width, next_height);
      layer.canvas.crop_x -= (int32_t)llround(old_width * origin_x_ratio);
      layer.canvas.crop_y -= (int32_t)llround(old_height * origin_y_ratio);
      layer.canvas.image_x -= (float)(old_width * origin_x_ratio);
      layer.canvas.image_y -= (float)(old_height * origin_y_ratio);
      // A baked camera is another layer in this canvas. Keep its absolute
      // canvas position stable when a Frame gesture moves the canvas origin,
      // matching the shared semantic rebase used at gesture commit.
      if (layer.overlay.camera_frame_width > 0) {
        layer.overlay.camera_frame_x -=
            (int32_t)llround(old_width * origin_x_ratio);
        layer.overlay.camera_frame_y -=
            (int32_t)llround(old_height * origin_y_ratio);
      }
      if (layer.overlay.cursor_width > 0) {
        layer.overlay.cursor_x -=
            (int32_t)llround(old_width * origin_x_ratio);
        layer.overlay.cursor_y -=
            (int32_t)llround(old_height * origin_y_ratio);
      }
      layer.canvas_width = next_width;
      layer.canvas_height = next_height;
      layer.canvas.background_radius = (uint32_t)MAX(
          llround(layer.canvas.background_radius * next_shortest / old_shortest), 0);
      layer.placement.width = (uint32_t)MAX(llround(layer.placement.width * width_ratio), 1);
      layer.placement.height = (uint32_t)MAX(llround(layer.placement.height * height_ratio), 1);
    }
    [resized addObject:[NSValue valueWithBytes:&layer
                                       objCType:@encode(ScreenwideWorkspaceLayer)]];
  }
  if (!found) return 0;
  presenter.workspaceLayers = resized;
  return 1;
}

int screenwide_gpu_still_presenter_update_workspace_selected_radius(
    void *handle, uint32_t selected_layer, double radius_percent) {
  if (handle == NULL || !isfinite(radius_percent)) return 0;
  ScreenwideStillPresenter *presenter = (__bridge ScreenwideStillPresenter *)handle;
  NSMutableArray<NSValue *> *updated =
      [NSMutableArray arrayWithCapacity:presenter.workspaceLayers.count];
  BOOL found = NO;
  for (NSValue *value in presenter.workspaceLayers) {
    ScreenwideWorkspaceLayer layer;
    [value getValue:&layer size:sizeof(layer)];
    if (layer.pane_index == selected_layer) {
      found = YES;
      double shortest = MIN(layer.canvas_width, layer.canvas_height);
      layer.canvas.background_radius = (uint32_t)MAX(
          llround(shortest * fmin(50.0, fmax(0.0, radius_percent)) / 100.0), 0);
    }
    [updated addObject:[NSValue valueWithBytes:&layer
                                       objCType:@encode(ScreenwideWorkspaceLayer)]];
  }
  if (!found) return 0;
  presenter.workspaceLayers = updated;
  return 1;
}

void screenwide_gpu_still_presenter_end_workspace_resize(
    void *handle, int commit) {
  if (handle == NULL) return;
  ScreenwideStillPresenter *presenter = (__bridge ScreenwideStillPresenter *)handle;
  if (commit == 0 && presenter.workspaceResizeLayers.count > 0)
    presenter.workspaceLayers = [presenter.workspaceResizeLayers mutableCopy];
  presenter.workspaceResizeLayers = nil;
}

int screenwide_gpu_still_presenter_present_workspace(
    void *handle, void *metal_layer, const ScreenwideWorkspaceLayer *layers,
    uint32_t layer_count, ScreenwidePresentBlock present) {
  if (handle == NULL || metal_layer == NULL || layers == NULL ||
      layer_count == 0 || present == NULL) return 0;
  @autoreleasepool {
    if (!screenwide_gpu_still_presenter_set_workspace(
            handle, layers, layer_count)) return 0;
    ScreenwideStillPresenter *presenter = (__bridge ScreenwideStillPresenter *)handle;
    return presenter_present_workspace_layers(
        presenter, (__bridge CAMetalLayer *)metal_layer, layers, layer_count,
        present);
  }
}

int screenwide_gpu_still_presenter_redraw_workspace(
    void *handle, void *metal_layer,
    const ScreenwideWorkspacePlacement *placements, uint32_t placement_count,
    const ScreenwideWorkspaceMagnifier *magnifier,
    ScreenwidePresentBlock present) {
  if (handle == NULL || metal_layer == NULL || placements == NULL ||
      placement_count == 0 || present == NULL) return 0;
  @autoreleasepool {
    ScreenwideStillPresenter *presenter = (__bridge ScreenwideStillPresenter *)handle;
    if (presenter.workspaceLayers.count != placement_count) return 0;
    ScreenwideWorkspaceLayer *layers = calloc(placement_count, sizeof(*layers));
    if (layers == NULL) return 0;
    for (uint32_t index = 0; index < placement_count; ++index) {
      [presenter.workspaceLayers[index] getValue:&layers[index]
                                            size:sizeof(layers[index])];
      layers[index].source_rgba = NULL;
      layers[index].placement = placements[index];
      NSNumber *key = @(layers[index].source_token);
      if (presenter.workspaceSources[key] == nil) {
        free(layers);
        return 0;
      }
    }
    // The cached source buffers are bound below; source_rgba is intentionally
    // null here so a redraw cannot accidentally copy from stale CPU memory.
    int result = 0;
    id<CAMetalDrawable> drawable = [(__bridge CAMetalLayer *)metal_layer nextDrawable];
    if (drawable != nil) {
      id<MTLCommandBuffer> command = [presenter.queue commandBuffer];
      MTLSize grid = MTLSizeMake(drawable.texture.width, drawable.texture.height, 1);
      id<MTLComputeCommandEncoder> clear = [command computeCommandEncoder];
      [clear setComputePipelineState:presenter.workspaceClearPipeline];
      [clear setTexture:drawable.texture atIndex:0];
      workspace_dispatch(clear, presenter.workspaceClearPipeline, grid);
      [clear endEncoding];
      for (uint32_t index = 0; index < placement_count; ++index) {
        id<MTLBuffer> source = presenter.workspaceSources[@(layers[index].source_token)];
        id<MTLBuffer> uniforms = [presenter.device newBufferWithBytes:&layers[index].canvas
            length:sizeof(layers[index].canvas) options:MTLResourceStorageModeShared];
        id<MTLBuffer> cursor = presenter.workspaceCursorSources[@(layers[index].source_token)]
            ?: [presenter.device newBufferWithLength:4 options:MTLResourceStorageModeShared];
        id<MTLBuffer> camera = presenter.workspaceCameraSources[@(layers[index].source_token)]
            ?: [presenter.device newBufferWithLength:4 options:MTLResourceStorageModeShared];
        id<MTLBuffer> overlay = [presenter.device newBufferWithBytes:&layers[index].overlay
            length:sizeof(layers[index].overlay) options:MTLResourceStorageModeShared];
        uint32_t dimensions[2] = {layers[index].source_width, layers[index].source_height};
        uint32_t first = index == 0 ? 1 : 0;
        id<MTLComputeCommandEncoder> encoder = [command computeCommandEncoder];
        [encoder setComputePipelineState:presenter.workspaceLayerPipeline];
        [encoder setBuffer:source offset:0 atIndex:0];
        [encoder setTexture:drawable.texture atIndex:0];
        [encoder setBuffer:uniforms offset:0 atIndex:1];
        [encoder setBytes:dimensions length:sizeof(dimensions) atIndex:2];
        [encoder setBytes:&layers[index].placement length:sizeof(layers[index].placement) atIndex:3];
        [encoder setBytes:&first length:sizeof(first) atIndex:4];
        uint32_t logical[2] = {layers[index].canvas_width,
                               layers[index].canvas_height};
        [encoder setBytes:logical length:sizeof(logical) atIndex:5];
        [encoder setBuffer:cursor offset:0 atIndex:6];
        [encoder setBuffer:camera offset:0 atIndex:7];
        [encoder setBuffer:overlay offset:0 atIndex:8];
        [encoder setBytes:&layers[index].seconds length:sizeof(layers[index].seconds) atIndex:9];
        workspace_dispatch(encoder, presenter.workspaceLayerPipeline, grid);
        [encoder endEncoding];
      }
      if (magnifier != NULL && magnifier->active != 0) {
        for (uint32_t index = 0; index < placement_count; ++index) {
          BOOL selectedLayer = magnifier->sample_camera != 0
              ? layers[index].pane_index == magnifier->pane_index
              : layers[index].layer_id == magnifier->layer_id;
          if (!selectedLayer) continue;
          NSNumber *token = @(layers[index].source_token);
          id<MTLBuffer> source = magnifier->sample_camera != 0
              ? presenter.workspaceCameraSources[token]
              : presenter.workspaceSources[token];
          uint32_t dimensions[2] = {
            magnifier->sample_camera != 0
                ? layers[index].overlay.camera_source_width
                : layers[index].source_width,
            magnifier->sample_camera != 0
                ? layers[index].overlay.camera_source_height
                : layers[index].source_height,
          };
          if (source == nil || dimensions[0] == 0 || dimensions[1] == 0) break;
          id<MTLComputeCommandEncoder> encoder = [command computeCommandEncoder];
          [encoder setComputePipelineState:presenter.workspaceMagnifierPipeline];
          [encoder setBuffer:source offset:0 atIndex:0];
          [encoder setTexture:drawable.texture atIndex:0];
          [encoder setBytes:dimensions length:sizeof(dimensions) atIndex:1];
          [encoder setBytes:magnifier length:sizeof(*magnifier) atIndex:2];
          workspace_dispatch(
              encoder, presenter.workspaceMagnifierPipeline,
              MTLSizeMake(MAX(magnifier->box_width, 1),
                          MAX(magnifier->box_height, 1), 1));
          [encoder endEncoding];
          break;
        }
      }
      present((__bridge void *)command, (__bridge void *)drawable);
      result = 1;
    }
    free(layers);
    return result;
  }
}

int screenwide_gpu_still_presenter_present(
    void *handle, void *metal_layer, uint64_t source_token,
    const uint8_t *source_rgba, uint32_t source_width, uint32_t source_height,
    const ScreenwideCanvas *canvas, double seconds, const uint8_t *cursor_rgba,
    const uint8_t *camera_rgba, const ScreenwideStillOverlay *overlay,
    ScreenwidePresentBlock present) {
  if (handle == NULL || metal_layer == NULL || source_rgba == NULL ||
      canvas == NULL || source_width == 0 || source_height == 0 ||
      present == NULL) return 0;
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
    present((__bridge void *)command, (__bridge void *)drawable);
    return 1;
  }
}

void screenwide_gpu_still_presenter_destroy(void *handle) {
  if (handle != NULL) CFBridgingRelease(handle);
}
