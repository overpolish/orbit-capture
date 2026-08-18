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
  uint32_t foreground_only;
} ScreenwideCanvas;

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
  int32_t camera_frame_x;
  int32_t camera_frame_y;
  uint32_t camera_frame_width;
  uint32_t camera_frame_height;
  uint32_t camera_radius;
  uint32_t camera_source_width;
  uint32_t camera_source_height;
  uint32_t camera_drop_shadow;
  uint32_t camera_on_top;
} ScreenwideStillOverlay;

/// Receives the encoded, uncommitted `MTLCommandBuffer` and its
/// `CAMetalDrawable`. The caller commits and presents so it can bind the
/// present to the Core Animation transaction that carries the pane's layout.
typedef void (^ScreenwidePresentBlock)(void *command_buffer, void *drawable);

/// A canvas' display rectangle in drawable pixels. Coordinates use a top-left
/// origin (the same convention as the workspace UI).
typedef struct {
  int32_t x;
  int32_t y;
  uint32_t width;
  uint32_t height;
} ScreenwideWorkspacePlacement;

typedef struct {
  uint32_t active;
  uint32_t pane_index;
  uint32_t layer_id;
  uint32_t sample_camera;
  uint32_t edges;
  uint32_t light_mode;
  float sample_u;
  float sample_v;
  int32_t box_x;
  int32_t box_y;
  uint32_t box_width;
  uint32_t box_height;
} ScreenwideWorkspaceMagnifier;

typedef struct {
  uint32_t index;
  double x;
  double y;
  double width;
  double height;
} ScreenwideWorkspacePaneRect;

/// One immutable RGBA source layer in the native workspace. `source_rgba` is
/// copied into a cached Metal buffer the first time its token is seen and is
/// never read back by the CPU during presentation.
typedef struct {
  uint32_t pane_index;
  uint32_t layer_id;
  const uint8_t *source_rgba;
  /// Optional CVPixelBufferRef. When source_kind is non-zero the presenter
  /// snapshots this buffer into its retained workspace source cache.
  void *source_pixels;
  uint32_t source_kind;
  uint64_t source_token;
  uint32_t source_width;
  uint32_t source_height;
  uint32_t canvas_width;
  uint32_t canvas_height;
  ScreenwideCanvas canvas;
  ScreenwideWorkspacePlacement placement;
  double seconds;
  const uint8_t *cursor_rgba;
  const uint8_t *camera_rgba;
  void *camera_pixels;
  ScreenwideStillOverlay overlay;
} ScreenwideWorkspaceLayer;

void *screenwide_gpu_still_presenter_create(void);
int screenwide_gpu_still_presenter_present(
    void *handle, void *metal_layer, uint64_t source_token,
    const uint8_t *source_rgba, uint32_t source_width, uint32_t source_height,
    const ScreenwideCanvas *canvas, double seconds, const uint8_t *cursor_rgba,
    const uint8_t *camera_rgba, const ScreenwideStillOverlay *overlay,
    ScreenwidePresentBlock present);
int screenwide_gpu_still_presenter_present_pixels(
    void *handle, void *metal_layer, uint64_t source_token,
    void *source_pixels, const ScreenwideCanvas *canvas, double seconds,
    const uint8_t *cursor_rgba, const uint8_t *camera_rgba,
    void *camera_pixels,
    const ScreenwideStillOverlay *overlay,
    ScreenwidePresentBlock present);
void screenwide_gpu_still_presenter_destroy(void *handle);

int screenwide_gpu_still_presenter_present_workspace(
    void *handle, void *metal_layer, const ScreenwideWorkspaceLayer *layers,
    uint32_t layer_count, ScreenwidePresentBlock present);

/// Replaces the retained workspace scene without acquiring a drawable. The
/// next native display-driven redraw presents this newest scene.
int screenwide_gpu_still_presenter_set_workspace(
    void *handle, const ScreenwideWorkspaceLayer *layers,
    uint32_t layer_count);

/// Updates one retained workspace layer's composition uniforms without
/// replacing its cached GPU source buffer.
int screenwide_gpu_still_presenter_workspace_source_size(
    void *handle, uint32_t pane_index, uint32_t *width, uint32_t *height);
int screenwide_gpu_still_presenter_workspace_canvas_size(
    void *handle, uint32_t pane_index, uint32_t *width, uint32_t *height);
int screenwide_gpu_still_presenter_workspace_camera_source_size(
    void *handle, uint32_t pane_index, uint32_t *width, uint32_t *height);
int screenwide_gpu_still_presenter_update_workspace_canvas(
    void *handle, uint32_t pane_index, uint32_t canvas_width,
    uint32_t canvas_height, const ScreenwideCanvas *canvas);
int screenwide_gpu_still_presenter_update_workspace_camera_overlay(
    void *handle, uint32_t pane_index, const ScreenwideStillOverlay *overlay);

/// Native Frame gestures transform the retained scene directly so the media
/// uniforms and OSC use the same revision before React mirrors the update.
int screenwide_gpu_still_presenter_begin_workspace_resize(void *handle);
int screenwide_gpu_still_presenter_update_workspace_resize(
    void *handle, double origin_x_ratio, double origin_y_ratio,
    double width_ratio, double height_ratio);
int screenwide_gpu_still_presenter_update_workspace_auto_fit_move(
    void *handle, uint32_t selected_layer, double move_x_ratio,
    double move_y_ratio, double origin_x_ratio, double origin_y_ratio,
    double width_ratio, double height_ratio);
int screenwide_gpu_still_presenter_update_recording_auto_fit_move(
    void *handle, uint32_t selected_pane, double move_x_ratio,
    double move_y_ratio, double origin_x_ratio, double origin_y_ratio,
    double width_ratio, double height_ratio);
/// Resizes only the selected retained recording layer. Unlike the screenshot
/// workspace resize this leaves every other pane's canvas and placement intact.
int screenwide_gpu_still_presenter_update_workspace_selected_resize(
    void *handle, uint32_t selected_layer, double origin_x_ratio,
    double origin_y_ratio, double width_ratio, double height_ratio);
int screenwide_gpu_still_presenter_update_workspace_selected_radius(
    void *handle, uint32_t selected_layer, double radius_percent);
void screenwide_gpu_still_presenter_end_workspace_resize(
    void *handle, int commit);

/// Re-renders the last workspace layer set using new drawable-pixel
/// placements. Pass one placement per retained layer, in the original order.
/// No source pointers are needed; the presenter uses its cached private
/// MTLBuffers. This is intended for native pan/zoom redraws.
int screenwide_gpu_still_presenter_redraw_workspace(
    void *handle, void *metal_layer,
    const ScreenwideWorkspacePlacement *placements, uint32_t placement_count,
    const ScreenwideWorkspaceMagnifier *magnifier,
    ScreenwidePresentBlock present);
