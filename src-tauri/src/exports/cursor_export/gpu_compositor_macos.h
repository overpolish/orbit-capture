// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <stdint.h>

typedef struct {
  float background_color[4];
  uint32_t background_radius;
  int32_t crop_x;
  int32_t crop_y;
  uint32_t crop_width;
  uint32_t crop_height;
  float image_x;
  float image_y;
  uint32_t image_width;
  uint32_t image_height;
  uint32_t radius;
  uint32_t drop_shadow;
  uint32_t mesh_enabled;
  uint32_t mesh_seed;
  float mesh_warp_percent;
  uint32_t mesh_point_count;
  float mesh_points[4][8];
  float mesh_colors[5][4];
  uint32_t clip_cursor_at_video_edge;
  uint32_t transparent_background;
} OrbitCanvas;

typedef struct {
  int32_t cursor_x;
  int32_t cursor_y;
  uint32_t cursor_width;
  uint32_t cursor_height;
  uint32_t cursor_source_width;
  uint32_t cursor_source_height;
  uint32_t camera_crop_x;
  uint32_t camera_crop_y;
  uint32_t camera_crop_width;
  uint32_t camera_crop_height;
  uint32_t camera_frame_x;
  uint32_t camera_frame_y;
  uint32_t camera_frame_width;
  uint32_t camera_frame_height;
  uint32_t camera_radius;
  uint32_t camera_source_width;
  uint32_t camera_source_height;
  uint32_t camera_drop_shadow;
} OrbitStillOverlay;

void *orbit_gpu_still_presenter_create(void);
int orbit_gpu_still_presenter_present(
    void *handle, void *metal_layer, uint64_t source_token,
    const uint8_t *source_rgba, uint32_t source_width, uint32_t source_height,
    const OrbitCanvas *canvas, double seconds, const uint8_t *cursor_rgba,
    const uint8_t *camera_rgba, const OrbitStillOverlay *overlay);
int orbit_gpu_still_presenter_present_pixels(
    void *handle, void *metal_layer, uint64_t source_token,
    void *source_pixels, const OrbitCanvas *canvas, double seconds,
    const uint8_t *cursor_rgba, const uint8_t *camera_rgba,
    void *camera_pixels,
    const OrbitStillOverlay *overlay);
void orbit_gpu_still_presenter_destroy(void *handle);
