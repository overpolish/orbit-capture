// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#import "recording_preview_surface_macos_private.h"

#include <math.h>

SCREENWIDE_PREVIEW_PRIVATE void remember_workspace_transform(
    ScreenwidePreviewSurface *surface, double width, double height);
SCREENWIDE_PREVIEW_PRIVATE void on_main_async(dispatch_block_t block);
SCREENWIDE_PREVIEW_PRIVATE void present_in_transaction(
    ScreenwidePreviewSurface *surface, ScreenwidePreviewView *view,
    id<MTLCommandBuffer> command, id<CAMetalDrawable> drawable);

/// Native retained-workspace extension point for timeline and screenshot tools.
static ScreenwidePresentBlock transaction_presenter(ScreenwidePreviewSurface *surface,
                                                    ScreenwidePreviewView *view) {
  return ^(void *command, void *drawable) {
    present_in_transaction(surface, view, (__bridge id<MTLCommandBuffer>)command,
                           (__bridge id<CAMetalDrawable>)drawable);
  };
}

/// Main thread only. Releases the workspace draw slot and re-runs the redraw
/// that was coalesced away while this one was in flight.
static void clear_workspace_draw_in_flight(ScreenwidePreviewSurface *surface) {
  [surface.workspaceLock lock];
  surface.workspaceDrawInFlight = NO;
  BOOL pending = surface.workspaceDrawPending;
  surface.workspaceDrawPending = NO;
  [surface.workspaceLock unlock];
  if (pending) redraw_workspace(surface);
}

static ScreenwidePresentBlock workspace_transaction_presenter(
    ScreenwidePreviewSurface *surface) {
  return ^(void *commandPointer, void *drawablePointer) {
    id<MTLCommandBuffer> command = (__bridge id<MTLCommandBuffer>)commandPointer;
    id<CAMetalDrawable> drawable = (__bridge id<CAMetalDrawable>)drawablePointer;
    // Encode pixels and OSC into one command, then use the same explicit Core
    // Animation transaction handoff as the proven multi-pane path. Direct
    // `presentDrawable` completed on the GPU quickly but could remain queued
    // for seconds before Core Animation displayed it.
    surface.workspaceEncodingCommand = command;
    surface.workspaceEncodingTexture = drawable.texture;
    redraw_selection(surface);
    surface.workspaceEncodingCommand = nil;
    surface.workspaceEncodingTexture = nil;
    ScreenwidePreviewView *workspace = surface.views.firstObject;
    // SAME-TURN CONSTRAINT: a workspace redraw acquires its drawable and
    // encodes on the main thread, so the present MUST happen in that same
    // runloop turn. Deferring it - to this command buffer's completed handler,
    // or to the batch's group-notify block - leaves the turn holding an
    // acquired-but-unpresented drawable of a `presentsWithTransaction` layer;
    // the turn's closing Core Animation flush then blocks waiting for a present
    // that is itself queued behind that flush on the main queue, and gives up
    // only at its ~1s watchdog. That is the measured 1-second hang. So hand the
    // buffer to `present_in_transaction` unconditionally: batched or not, its
    // main-thread path commits, waits for scheduled and presents inline here.
    //
    // Registered before the commit that `present_in_transaction` performs -
    // Metal rejects completed handlers added after `commit`. This clear re-arms
    // `workspaceDrawPending` coalescing, so it must stay.
    [command addCompletedHandler:^(__unused id<MTLCommandBuffer> completed) {
      dispatch_async(dispatch_get_main_queue(), ^{
        clear_workspace_draw_in_flight(surface);
      });
    }];
    present_in_transaction(surface, workspace, command, drawable);
  };
}

static ScreenwideWorkspacePlacement workspace_placement(
    ScreenwidePreviewSurface *surface) {
  if (surface.editorBaseRects.count == 0)
    return (ScreenwideWorkspacePlacement){0};
  NSRect transformed = editor_frame(
      surface, surface.editorBaseRects[0].rectValue);
  CGFloat scale = surface.host.window.backingScaleFactor ?: 1.0;
  CGFloat top = surface.container.bounds.size.height - NSMaxY(transformed);
  return (ScreenwideWorkspacePlacement){
    (int32_t)llround(transformed.origin.x * scale),
    (int32_t)llround(top * scale),
    (uint32_t)MAX(llround(transformed.size.width * scale), 1),
    (uint32_t)MAX(llround(transformed.size.height * scale), 1),
  };
}

SCREENWIDE_PREVIEW_PRIVATE void update_crop_magnifier(ScreenwidePreviewSurface *surface,
                                  NSPoint point, uint32_t edges) {
  if (!surface.workspaceMode || !surface.hasSelection ||
      surface.selection.crop_mode == 0 ||
      surface.selection.pane_index >= surface.editorBaseRects.count) {
    ScreenwideWorkspaceMagnifier clearedMagnifier = surface.workspaceMagnifier;
    clearedMagnifier.active = 0;
    surface.workspaceMagnifier = clearedMagnifier;
    return;
  }
  NSRect transformed = editor_frame(
      surface, surface.editorBaseRects[surface.selection.pane_index].rectValue);
  // The interaction view is flipped (top-left origin), while editor frames
  // use AppKit's bottom-left coordinates. The magnifier and source mapping
  // both operate in the interaction view's top-left coordinate space.
  NSRect pane = NSMakeRect(
      transformed.origin.x,
      surface.interaction.bounds.size.height - NSMaxY(transformed),
      transformed.size.width, transformed.size.height);
  if (NSIsEmptyRect(pane) || surface.selection.image_width <= 0.0 ||
      surface.selection.image_height <= 0.0) {
    ScreenwideWorkspaceMagnifier clearedMagnifier = surface.workspaceMagnifier;
    clearedMagnifier.active = 0;
    surface.workspaceMagnifier = clearedMagnifier;
    return;
  }
  double paneX = (point.x - NSMinX(pane)) / pane.size.width;
  double paneY = (point.y - NSMinY(pane)) / pane.size.height;
  double sampleU = (paneX - surface.selection.image_x) /
                   surface.selection.image_width;
  double sampleV = (paneY - surface.selection.image_y) /
                   surface.selection.image_height;
  CGFloat scale = surface.host.window.backingScaleFactor ?: 1.0;
  int32_t size = (int32_t)MAX(llround(96.0 * scale), 1);
  int32_t centerX = (int32_t)llround(point.x * scale);
  int32_t centerY = (int32_t)llround(point.y * scale);
  // Keep the box centred on the crop edge even when part of it falls outside
  // the workarea. Clamping its origin would move the visual split away from
  // the pointer/sample coordinate; the compute kernel safely clips writes to
  // the drawable while retaining this local coordinate system.
  int32_t boxX = centerX - size / 2;
  int32_t boxY = centerY - size / 2;
  surface.workspaceMagnifier = (ScreenwideWorkspaceMagnifier){
    .active = 1,
    .pane_index = surface.selection.pane_index,
    .layer_id = surface.selection.layer_id,
    .sample_camera = surface.workspaceExplicitPlacements &&
                     surface.selection.layer_id != surface.selection.pane_index,
    .edges = edges,
    .light_mode = [[[surface.interaction effectiveAppearance]
        bestMatchFromAppearancesWithNames:@[NSAppearanceNameAqua,
                                            NSAppearanceNameDarkAqua]]
        isEqualToString:NSAppearanceNameAqua] ? 1 : 0,
    .sample_u = (float)fmin(1.0, fmax(0.0, sampleU)),
    .sample_v = (float)fmin(1.0, fmax(0.0, sampleV)),
    .box_x = boxX,
    .box_y = boxY,
    .box_width = (uint32_t)size,
    .box_height = (uint32_t)size,
  };
}

SCREENWIDE_PREVIEW_PRIVATE void begin_workspace_frame_resize(ScreenwidePreviewSurface *surface) {
  if (!surface.workspaceMode || surface.views.count == 0) return;
  ScreenwidePreviewView *workspace = surface.views[0];
  remember_workspace_transform(surface, surface.workspaceNaturalWidth,
                               surface.workspaceNaturalHeight);
  surface.workspaceResizeNaturalWidth = surface.workspaceNaturalWidth;
  surface.workspaceResizeNaturalHeight = surface.workspaceNaturalHeight;
  [surface.workspaceLock lock];
  screenwide_gpu_still_presenter_begin_workspace_resize(workspace.compositor);
  [surface.workspaceLock unlock];
}

SCREENWIDE_PREVIEW_PRIVATE void update_workspace_frame_resize(
    ScreenwidePreviewSurface *surface, NSRect start, NSRect resized) {
  if (!surface.workspaceMode || surface.views.count == 0 ||
      start.size.width <= 0.0 || start.size.height <= 0.0) return;
  ScreenwidePreviewView *workspace = surface.views[0];
  double originX = (resized.origin.x - start.origin.x) / start.size.width;
  double originY = (resized.origin.y - start.origin.y) / start.size.height;
  double width = resized.size.width / start.size.width;
  double height = resized.size.height / start.size.height;
  surface.workspaceNaturalWidth = surface.workspaceResizeNaturalWidth * width;
  surface.workspaceNaturalHeight = surface.workspaceResizeNaturalHeight * height;
  [surface.workspaceLock lock];
  screenwide_gpu_still_presenter_update_workspace_resize(
      workspace.compositor, originX, originY, width, height);
  [surface.workspaceLock unlock];
}

SCREENWIDE_PREVIEW_PRIVATE BOOL update_workspace_auto_fit_move(
    ScreenwidePreviewSurface *surface, uint32_t selected_layer,
    double move_x, double move_y, NSRect start, NSRect resized) {
  if (!surface.workspaceMode || surface.views.count == 0 ||
      start.size.width <= 0.0 || start.size.height <= 0.0) return NO;
  ScreenwidePreviewView *workspace = surface.views[0];
  double originX = (resized.origin.x - start.origin.x) / start.size.width;
  double originY = (resized.origin.y - start.origin.y) / start.size.height;
  double width = resized.size.width / start.size.width;
  double height = resized.size.height / start.size.height;
  surface.workspaceNaturalWidth = surface.workspaceResizeNaturalWidth * width;
  surface.workspaceNaturalHeight = surface.workspaceResizeNaturalHeight * height;
  [surface.workspaceLock lock];
  int result = screenwide_gpu_still_presenter_update_workspace_auto_fit_move(
      workspace.compositor, selected_layer, move_x, move_y,
      originX, originY, width, height);
  [surface.workspaceLock unlock];
  return result != 0;
}

SCREENWIDE_PREVIEW_PRIVATE void end_workspace_frame_resize(
    ScreenwidePreviewSurface *surface, BOOL commit) {
  if (!surface.workspaceMode || surface.views.count == 0) return;
  ScreenwidePreviewView *workspace = surface.views[0];
  [surface.workspaceLock lock];
  screenwide_gpu_still_presenter_end_workspace_resize(
      workspace.compositor, commit ? 1 : 0);
  if (!commit) {
    surface.workspaceNaturalWidth = surface.workspaceResizeNaturalWidth;
    surface.workspaceNaturalHeight = surface.workspaceResizeNaturalHeight;
  } else {
    remember_workspace_transform(surface, surface.workspaceNaturalWidth,
                                 surface.workspaceNaturalHeight);
  }
  [surface.workspaceLock unlock];
}

SCREENWIDE_PREVIEW_PRIVATE void redraw_workspace(ScreenwidePreviewSurface *surface) {
  if (!surface.workspaceMode || surface.workspaceLayerCount == 0 ||
      surface.views.count == 0) return;
  ScreenwidePreviewView *workspace = surface.views[0];
  if (!workspace.active || workspace.hidden) return;
  [surface.workspaceLock lock];
  if (surface.workspaceDrawInFlight) {
    surface.workspaceDrawPending = YES;
    [surface.workspaceLock unlock];
    return;
  }
  surface.workspaceDrawInFlight = YES;
  ScreenwideWorkspacePlacement placement = workspace_placement(surface);
  NSMutableData *data = [NSMutableData
      dataWithLength:sizeof(placement) * surface.workspaceLayerCount];
  ScreenwideWorkspacePlacement *placements = data.mutableBytes;
  if (surface.workspaceExplicitPlacements &&
      surface.workspacePlacements.length >= sizeof(placement) * surface.workspaceLayerCount) {
    memcpy(placements, surface.workspacePlacements.bytes,
           sizeof(placement) * surface.workspaceLayerCount);
    CGFloat scale = surface.host.window.backingScaleFactor ?: 1.0;
    for (uint32_t index = 0; index < surface.workspaceLayerCount; index++) {
      uint32_t pane_index = surface.workspacePaneIndices[index].unsignedIntValue;
      if (pane_index >= surface.editorBaseRects.count ||
          ![surface.workspaceActivePaneIndices containsObject:@(pane_index)]) {
        placements[index] = (ScreenwideWorkspacePlacement){0};
        continue;
      }
      NSRect transformed = editor_frame(surface,
                                        surface.editorBaseRects[pane_index].rectValue);
      placements[index] = (ScreenwideWorkspacePlacement){
        (int32_t)llround(transformed.origin.x * scale),
        (int32_t)llround((surface.container.bounds.size.height - NSMaxY(transformed)) * scale),
        (uint32_t)MAX(llround(transformed.size.width * scale), 1),
        (uint32_t)MAX(llround(transformed.size.height * scale), 1),
      };
    }
  } else {
    for (uint32_t index = 0; index < surface.workspaceLayerCount; index++)
      placements[index] = placement;
  }
  CAMetalLayer *layer = (CAMetalLayer *)workspace.layer;
  ScreenwideWorkspaceMagnifier magnifier = surface.workspaceMagnifier;
  int result = screenwide_gpu_still_presenter_redraw_workspace(
      workspace.compositor, (__bridge void *)layer, placements,
      surface.workspaceLayerCount, &magnifier,
      workspace_transaction_presenter(surface));
  if (result == 0) surface.workspaceDrawInFlight = NO;
  [surface.workspaceLock unlock];
}

int screenwide_preview_surface_present_screenshot_workspace(
    void *handle, const ScreenwideWorkspaceLayer *layers,
    uint32_t layer_count) {
  if (handle == NULL || layers == NULL || layer_count == 0) return 0;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  if (!surface.workspaceMode || surface.views.count == 0) return 0;
  ScreenwidePreviewView *workspace = surface.views[0];
  // The pane is made active by the layout block queued on the main thread; a
  // present that arrives before it has run is reported as not staged so the
  // caller can come back once the pane exists.
  if (!workspace.active) return 0;
  ScreenwideWorkspacePlacement placement = workspace_placement(surface);
  ScreenwideWorkspaceLayer *placed = calloc(layer_count, sizeof(*placed));
  if (placed == NULL) return 0;
  for (uint32_t index = 0; index < layer_count; index++) {
    placed[index] = layers[index];
    placed[index].placement = placement;
  }
  [surface.workspaceLock lock];
  int staged = screenwide_gpu_still_presenter_set_workspace(
      workspace.compositor, placed, layer_count);
  free(placed);
  if (staged == 0) {
    [surface.workspaceLock unlock];
    return 0;
  }
  surface.workspaceLayerCount = layer_count;
  surface.workspaceExplicitPlacements = NO;
  surface.workspacePlacements = nil;
  surface.workspacePaneIndices = nil;
  BOOL drawInFlight = surface.workspaceDrawInFlight;
  if (drawInFlight)
    surface.workspaceDrawPending = YES;
  [surface.workspaceLock unlock];
  if (!drawInFlight) {
    dispatch_async(dispatch_get_main_queue(), ^{
      workspace.hidden = NO;
      redraw_workspace(surface);
    });
  }
  return 1;
}

/// Presents a retained recording scene whose layers already contain explicit
/// workspace placements. Unlike the screenshot helper, placements are not
/// collapsed to the primary canvas: unbaked screen/camera panes remain
/// independently editable inside one native drawable.
int screenwide_preview_surface_present_recording_workspace(
    void *handle, const ScreenwideWorkspaceLayer *layers,
    uint32_t layer_count) {
  if (handle == NULL || layers == NULL || layer_count == 0) return 0;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  if (!surface.workspaceMode || surface.views.count == 0) return 0;
  ScreenwidePreviewView *workspace = surface.views[0];
  if (!workspace.active) return 1;
  [surface.workspaceLock lock];
  int staged = screenwide_gpu_still_presenter_set_workspace(
      workspace.compositor, layers, layer_count);
  if (staged != 0) {
    surface.workspaceLayerCount = layer_count;
    surface.workspaceExplicitPlacements = YES;
    NSMutableArray<NSNumber *> *paneIndices = [NSMutableArray arrayWithCapacity:layer_count];
    for (uint32_t index = 0; index < layer_count; index++)
      [paneIndices addObject:@(layers[index].pane_index)];
    surface.workspacePaneIndices = paneIndices;
    surface.workspacePlacements = [NSMutableData
        dataWithLength:sizeof(ScreenwideWorkspacePlacement) * layer_count];
    ScreenwideWorkspacePlacement *placements = surface.workspacePlacements.mutableBytes;
    for (uint32_t index = 0; index < layer_count; index++)
      placements[index] = layers[index].placement;
  }
  BOOL drawInFlight = surface.workspaceDrawInFlight;
  if (drawInFlight) surface.workspaceDrawPending = YES;
  [surface.workspaceLock unlock];
  if (staged == 0) return 0;
  if (!drawInFlight) {
    dispatch_async(dispatch_get_main_queue(), ^{
      workspace.hidden = NO;
      redraw_workspace(surface);
    });
  }
  return 1;
}

int screenwide_preview_surface_workspace_source_size(
    void *handle, uint32_t pane_index, uint32_t *width, uint32_t *height) {
  if (handle == NULL) return 0;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  if (!surface.workspaceMode || surface.views.count == 0) return 0;
  ScreenwidePreviewView *workspace = surface.views[0];
  [surface.workspaceLock lock];
  int result = screenwide_gpu_still_presenter_workspace_source_size(
      workspace.compositor, pane_index, width, height);
  [surface.workspaceLock unlock];
  return result;
}

int screenwide_preview_surface_workspace_canvas_size(
    void *handle, uint32_t pane_index, uint32_t *width, uint32_t *height) {
  if (handle == NULL) return 0;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  if (!surface.workspaceMode || surface.views.count == 0) return 0;
  ScreenwidePreviewView *workspace = surface.views[0];
  [surface.workspaceLock lock];
  int result = screenwide_gpu_still_presenter_workspace_canvas_size(
      workspace.compositor, pane_index, width, height);
  [surface.workspaceLock unlock];
  return result;
}

int screenwide_preview_surface_workspace_camera_source_size(
    void *handle, uint32_t pane_index, uint32_t *width, uint32_t *height) {
  if (handle == NULL) return 0;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  if (!surface.workspaceMode || surface.views.count == 0) return 0;
  ScreenwidePreviewView *workspace = surface.views[0];
  [surface.workspaceLock lock];
  int result = screenwide_gpu_still_presenter_workspace_camera_source_size(
      workspace.compositor, pane_index, width, height);
  [surface.workspaceLock unlock];
  return result;
}

int screenwide_preview_surface_update_workspace_canvas(
    void *handle, uint32_t pane_index, uint32_t canvas_width,
    uint32_t canvas_height, const ScreenwideCanvas *canvas) {
  if (handle == NULL || canvas == NULL) return 0;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  if (!surface.workspaceMode || surface.views.count == 0) return 0;
  ScreenwidePreviewView *workspace = surface.views[0];
  [surface.workspaceLock lock];
  int result = screenwide_gpu_still_presenter_update_workspace_canvas(
      workspace.compositor, pane_index, canvas_width, canvas_height, canvas);
  [surface.workspaceLock unlock];
  return result;
}

int screenwide_preview_surface_update_workspace_camera_overlay(
    void *handle, uint32_t pane_index, const ScreenwideStillOverlay *overlay) {
  if (handle == NULL || overlay == NULL) return 0;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  if (!surface.workspaceMode || surface.views.count == 0) return 0;
  ScreenwidePreviewView *workspace = surface.views[0];
  [surface.workspaceLock lock];
  int result = screenwide_gpu_still_presenter_update_workspace_camera_overlay(
      workspace.compositor, pane_index, overlay);
  [surface.workspaceLock unlock];
  return result;
}

int screenwide_preview_surface_redraw_workspace(void *handle) {
  if (handle == NULL) return 0;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  if (!surface.workspaceMode || surface.views.count == 0) return 0;
  // The draw reads the pane geometry that the layout setters now apply
  // asynchronously, so it has to queue behind them instead of racing them
  // from the caller's thread. The result still reports what it always did:
  // whether a workspace pane exists to draw into.
  on_main_async(^{
    redraw_workspace(surface);
  });
  return 1;
}

int screenwide_preview_surface_update_workspace_selected_resize(
    void *handle, uint32_t selected_layer, double origin_x_ratio,
    double origin_y_ratio, double width_ratio, double height_ratio) {
  if (handle == NULL) return 0;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  if (!surface.workspaceMode || surface.views.count == 0) return 0;
  ScreenwidePreviewView *workspace = surface.views[0];
  [surface.workspaceLock lock];
  int result = screenwide_gpu_still_presenter_update_workspace_selected_resize(
      workspace.compositor, selected_layer, origin_x_ratio, origin_y_ratio,
      width_ratio, height_ratio);
  [surface.workspaceLock unlock];
  if (result != 0) redraw_workspace(surface);
  return result;
}

int screenwide_preview_surface_present_composed(
    void *handle, uint32_t index, uint64_t source_token,
    const uint8_t *source_rgba, uint32_t source_width, uint32_t source_height,
    uint32_t output_width, uint32_t output_height,
    const ScreenwideCanvas *canvas, double seconds, const uint8_t *cursor_rgba,
    const uint8_t *camera_rgba, const ScreenwideStillOverlay *overlay) {
  if (handle == NULL || source_rgba == NULL || canvas == NULL) return 0;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  // A composed frame is only ready once it was handed to a live pane. A
  // missing pane must be retried after layout creates it.
  if (index >= surface.views.count) return 0;
  ScreenwidePreviewView *view = surface.views[index];
  if (!view.active) return 1;
  CAMetalLayer *layer = (CAMetalLayer *)view.layer;
  layer.drawableSize = CGSizeMake(MAX(output_width, 2u), MAX(output_height, 2u));
  return screenwide_gpu_still_presenter_present(
      view.compositor, (__bridge void *)layer, source_token, source_rgba,
      source_width, source_height, canvas, seconds, cursor_rgba, camera_rgba,
      overlay, transaction_presenter(surface, view));
}

int screenwide_preview_surface_present_composed_pixels(
    void *handle, uint32_t index, uint64_t source_token, void *source_pixels,
    uint32_t output_width, uint32_t output_height, const ScreenwideCanvas *canvas,
    double seconds, const uint8_t *cursor_rgba, const uint8_t *camera_rgba,
    void *camera_pixels,
    const ScreenwideStillOverlay *overlay) {
  if (handle == NULL || source_pixels == NULL || canvas == NULL) return 0;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  if (index >= surface.views.count) return 0;
  ScreenwidePreviewView *view = surface.views[index];
  if (!view.active) return 1;
  CAMetalLayer *layer = (CAMetalLayer *)view.layer;
  layer.drawableSize = CGSizeMake(MAX(output_width, 2u), MAX(output_height, 2u));
  return screenwide_gpu_still_presenter_present_pixels(
      view.compositor, (__bridge void *)layer, source_token, source_pixels,
      canvas, seconds, cursor_rgba, camera_rgba, camera_pixels, overlay,
      transaction_presenter(surface, view));
}
