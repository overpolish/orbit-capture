// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#import "recording_preview_surface_macos_private.h"

#include <math.h>

SCREENWIDE_PREVIEW_PRIVATE void restore_workspace_transform(
    ScreenwidePreviewSurface *surface, double width, double height);
SCREENWIDE_PREVIEW_PRIVATE void on_main_async(dispatch_block_t block);

/// Native scene-layout extension point for future timeline topology changes.
static ScreenwidePreviewView *make_view(ScreenwidePreviewSurface *surface) {
  ScreenwidePreviewView *view = [[ScreenwidePreviewView alloc] initWithFrame:NSZeroRect];
  view.wantsLayer = YES;
  CAMetalLayer *layer = [CAMetalLayer layer];
  layer.device = surface.device;
  layer.pixelFormat = MTLPixelFormatBGRA8Unorm;
  layer.framebufferOnly = NO;
  layer.displaySyncEnabled = YES;
  // Presents ride the Core Animation transaction, so a pane's new frame and
  // its freshly composed drawable reach the screen in the same commit instead
  // of racing each other across two display ticks (see `present_in_transaction`).
  layer.presentsWithTransaction = YES;
  // Never stretch an old drawable while a fast canvas resize prepares its
  // replacement. The presenter swaps in the correctly sized frame next.
  layer.contentsGravity = kCAGravityResizeAspect;
  layer.opaque = NO;
  // Composed frames are sRGB. Tagging the layer makes Core Animation colour
  // match them exactly like the webview matches its canvases, so the native
  // panes and the editing canvases render identical colours.
  CGColorSpaceRef srgb = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
  layer.colorspace = srgb;
  CGColorSpaceRelease(srgb);
  view.layer = layer;
  view.compositor = screenwide_gpu_still_presenter_create();
  view.hidden = YES;
  view.active = NO;
  [surface.container addSubview:view positioned:NSWindowAbove relativeTo:nil];
  return view;
}

void screenwide_preview_surface_set_viewport(void *handle,
                                        double x, double y,
                                        double width, double height,
                                        double red, double green, double blue,
                                        double alpha) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  on_main_async(^{
    CGFloat host_height = surface.host.bounds.size.height;
    NSRect nextFrame = NSMakeRect(x, host_height - y - height, width, height);
    if (!NSEqualRects(surface.interaction.frame, nextFrame)) {
      surface.selectionDrawRevision += 1;
      surface.selectionLayer.hidden = YES;
    }
    surface.container.frame = nextFrame;
    surface.interaction.frame = surface.container.frame;
    // An opaque backstop: while the webview's mask holes and the native pane
    // layout briefly disagree (pan, zoom, resize), the gap shows the app's
    // dark backdrop instead of seeing through the window.
    surface.container.layer.backgroundColor =
        CGColorCreateSRGB(red, green, blue, alpha);
    // The webview punches the whole viewport out of its backdrop, so the
    // backstop must be there from the first layout on, not only from the
    // first presented frame. The panes themselves stay hidden until then.
    if (width > 0 && height > 0) surface.container.hidden = NO;
    if (surface.editorEnabled && width > 0 && height > 0)
      surface.interaction.hidden = NO;
    invalidate_selection_cursor_rects(surface);
  });
}

void screenwide_preview_surface_enable_editor(
    void *handle, screenwide_preview_transform_callback callback,
    void *context) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  on_main_async(^{
    surface.editorEnabled = callback != NULL;
    surface.transformCallback = callback;
    surface.transformContext = context;
    surface.interaction.hidden = !surface.editorEnabled;
    if (!surface.editorEnabled) {
      [surface.interaction releaseCursorControl];
      surface.editorPanX = 0;
      surface.editorPanY = 0;
      surface.editorZoom = 1.0;
    }
    redraw_selection(surface);
  });
}

void screenwide_preview_surface_set_selection_gesture_callback(
    void *handle, screenwide_preview_selection_gesture_callback callback,
    void *context) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  on_main_async(^{
    surface.selectionGestureCallback = callback;
    surface.selectionGestureContext = context;
    invalidate_selection_cursor_rects(surface);
  });
}

void screenwide_preview_surface_set_selection_callback(
    void *handle, screenwide_preview_selection_callback callback,
    void *context) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  on_main_async(^{
    surface.selectionCallback = callback;
    surface.selectionContext = context;
  });
}

void screenwide_preview_surface_set_selection_targets(
    void *handle, const ScreenwidePreviewSelection *targets, size_t count,
    int enabled) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  // The block outlives this call, so the caller's array is copied here, while
  // it is still alive, and only the copy is captured.
  NSMutableArray<NSValue *> *copied = [NSMutableArray arrayWithCapacity:count];
  for (size_t index = 0; index < count; index++)
    [copied addObject:[NSValue valueWithBytes:&targets[index]
                                     objCType:@encode(ScreenwidePreviewSelection)]];
  on_main_async(^{
    surface.selectionTargets = copied;
    surface.selectionHitTestingEnabled = enabled != 0;
  });
}

void screenwide_preview_surface_set_selection_snapping(void *handle,
                                                        int enabled) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  on_main_async(^{
    surface.selectionSnappingEnabled = enabled != 0;
    if (!surface.selectionSnappingEnabled) {
      clear_selection_snap_guides(surface);
      redraw_selection(surface);
    }
  });
}

void screenwide_preview_surface_set_editor_zoom(void *handle,
                                                double zoom_percent) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  on_main_async(^{
    if (!surface.editorEnabled) return;
    NSPoint center = NSMakePoint(NSMidX(surface.interaction.bounds),
                                 NSMidY(surface.interaction.bounds));
    set_editor_zoom(surface, zoom_percent / 100.0, center);
  });
}

void screenwide_preview_surface_center_editor(void *handle) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  on_main_async(^{
    if (!surface.editorEnabled) return;
    surface.editorPanX = 0.0;
    surface.editorPanY = 0.0;
    apply_editor_transform(surface);
  });
}

void screenwide_preview_surface_set_selection(void *handle,
                                              const ScreenwidePreviewSelection *selection) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  // The block outlives this call, so the caller's selection is copied by
  // value here and the block reads only the copy.
  const BOOL hasSelection = selection != NULL;
  const ScreenwidePreviewSelection copy =
      hasSelection ? *selection : (ScreenwidePreviewSelection){0};
  on_main_async(^{
    if (surface.interaction.selectionDragActive && !hasSelection) {
      if (surface.interaction.selectionDragOperation == 3 ||
          (surface.interaction.selectionDragOperation == 0 &&
           !NSIsEmptyRect(surface.interaction.selectionMoveFrameStart)))
        end_workspace_frame_resize(surface, NO);
      if (surface.interaction.selectionDragOperation == 0 &&
          !NSIsEmptyRect(surface.interaction.selectionMoveFrameStart)) {
        for (NSUInteger index = 0;
             index < surface.editorBaseRects.count; index++)
          surface.editorBaseRects[index] =
              [NSValue valueWithRect:surface.interaction.selectionMoveFrameStart];
        surface.editorZoom = surface.interaction.selectionMoveZoomStart;
        surface.editorPanX = surface.interaction.selectionMovePanStart.x;
        surface.editorPanY = surface.interaction.selectionMovePanStart.y;
      }
      surface.interaction.selectionDragActive = NO;
      surface.interaction.selectionMoveFrameStart = NSZeroRect;
      surface.interaction.selectionMoveAutoFitActive = NO;
      surface.interaction.selectionMoveTargetsStart = nil;
      surface.interaction.panning = NO;
      surface.hasSelection = NO;
      ScreenwideWorkspaceMagnifier clearedMagnifier = surface.workspaceMagnifier;
      clearedMagnifier.active = 0;
      surface.workspaceMagnifier = clearedMagnifier;
      surface.selectionLayer.hidden = YES;
      emit_selection_gesture(surface, 3, surface.interaction.selectionDragOperation,
                             surface.interaction.selectionDragEdges, 1.0, 0.0, 0.0);
      return;
    }
    if (surface.interaction.selectionDragActive && hasSelection) return;
    // Layout commands update selection, viewport and pane frames as one
    // logical scene. Drawing here would briefly apply the new normalized OSC
    // to the previous split/baked pane geometry; finish_layout draws it once
    // every base rect belongs to the same scene.
    BOOL topologyChanged = surface.hasSelection != hasSelection;
    BOOL changed = topologyChanged;
    if (!changed && hasSelection) {
      topologyChanged = surface.selection.pane_index != copy.pane_index ||
                        surface.selection.layer_id != copy.layer_id;
      changed = topologyChanged ||
                surface.selection.x != copy.x ||
                surface.selection.y != copy.y ||
                surface.selection.width != copy.width ||
                surface.selection.height != copy.height;
    }
    if (changed) {
      surface.selectionDrawRevision += 1;
      if (topologyChanged) surface.selectionLayer.hidden = YES;
    }
    surface.hasSelection = hasSelection;
    if (hasSelection) surface.selection = copy;
    invalidate_selection_cursor_rects(surface);
  });
}

void screenwide_preview_surface_set_selection_visible(void *handle,
                                                      int visible) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  on_main_async(^{
    surface.selectionVisible = visible != 0;
    redraw_selection(surface);
  });
}

void screenwide_preview_surface_begin_layout(void *handle) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  on_main_async(^{
    for (ScreenwidePreviewView *view in surface.views) view.active = NO;
  });
}

void screenwide_preview_surface_layout(void *handle, uint32_t index,
                                  double x, double y, double width, double height,
                                  int defer_resize) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  on_main_async(^{
    surface.workspaceMode = NO;
    while (surface.views.count <= index) [surface.views addObject:make_view(surface)];
    ScreenwidePreviewView *view = surface.views[index];
    CGFloat viewport_height = surface.container.bounds.size.height;
    NSRect base = NSMakeRect(x, y, width, height);
    while (surface.editorBaseRects.count <= index)
      [surface.editorBaseRects addObject:[NSValue valueWithRect:NSZeroRect]];
    BOOL ownsFrameDuringGesture =
        surface.interaction.selectionDragActive &&
        surface.interaction.selectionDragOperation == 3;
    if (ownsFrameDuringGesture)
      base = surface.editorBaseRects[index].rectValue;
    else
      surface.editorBaseRects[index] = [NSValue valueWithRect:base];
    NSRect frame = NSMakeRect(x, viewport_height - y - height, width, height);
    if (surface.editorEnabled) {
      frame = editor_frame(surface, base);
    }
    // With a present on the way the frame waits for it, so every pane's rect
    // and its pixels change in that one commit (a pane that only moves would
    // otherwise shift a tick before its neighbour that also resized). A pan
    // with no present coming applies at once; a hidden pane has nothing
    // stale to show and needs no such care.
    if (defer_resize && !view.hidden) {
      view.pendingFrame = frame;
      view.hasPendingFrame = YES;
    } else {
      view.frame = frame;
      view.hasPendingFrame = NO;
    }
    view.active = YES;
    CGFloat scale = surface.host.window.backingScaleFactor ?: 1.0;
    CAMetalLayer *layer = (CAMetalLayer *)view.layer;
    layer.contentsScale = scale;
  });
}

void screenwide_preview_surface_layout_workspace(
    void *handle, double x, double y, double width, double height,
    double natural_width, double natural_height, int defer_draw) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  on_main_async(^{
    surface.workspaceMode = YES;
    surface.workspaceActivePaneIndices = [NSSet setWithObject:@0];
    surface.workspaceLayoutAwaitsPresent = defer_draw != 0;
    while (surface.views.count == 0)
      [surface.views addObject:make_view(surface)];
    ScreenwidePreviewView *workspace = surface.views[0];
    while (surface.editorBaseRects.count == 0)
      [surface.editorBaseRects addObject:[NSValue valueWithRect:NSZeroRect]];
    BOOL ownsFrame = surface.interaction.selectionDragActive &&
        (surface.interaction.selectionDragOperation == 3 ||
         (surface.interaction.selectionDragOperation == 0 &&
          !NSIsEmptyRect(surface.interaction.selectionMoveFrameStart)));
    if (!ownsFrame) {
      NSRect incoming = NSMakeRect(x, y, width, height);
      BOOL naturalSizeChanged = surface.workspaceNaturalWidth > 0.0 &&
          (fabs(surface.workspaceNaturalWidth - natural_width) > 0.51 ||
           fabs(surface.workspaceNaturalHeight - natural_height) > 0.51);
      if (naturalSizeChanged)
        restore_workspace_transform(surface, natural_width, natural_height);
      // The first layout after a commit echoes that commit; anything later
      // is a genuine change (undo/redo) and may restore again.
      surface.keepTransformForCommittedNaturalSize = NO;
      surface.editorBaseRects[0] = [NSValue valueWithRect:incoming];
      surface.workspaceNaturalWidth = natural_width;
      surface.workspaceNaturalHeight = natural_height;
    }
    workspace.frame = surface.container.bounds;
    workspace.hasPendingFrame = NO;
    workspace.active = YES;
    CGFloat scale = surface.host.window.backingScaleFactor ?: 1.0;
    CAMetalLayer *layer = (CAMetalLayer *)workspace.layer;
    layer.contentsScale = scale;
    layer.presentsWithTransaction = YES;
    NSSize size = surface.container.bounds.size;
    layer.drawableSize = CGSizeMake(MAX(size.width * scale, 2.0),
                                    MAX(size.height * scale, 2.0));
    for (NSUInteger index = 1; index < surface.views.count; index++) {
      surface.views[index].active = NO;
      surface.views[index].hidden = YES;
      surface.views[index].hasPendingFrame = NO;
    }
  });
}

void screenwide_preview_surface_layout_recording_workspace(
    void *handle, double x, double y, double width, double height,
    double natural_width, double natural_height,
    const ScreenwideWorkspacePaneRect *panes, uint32_t pane_count,
    int defer_draw) {
  if (handle == NULL || panes == NULL || pane_count == 0) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  // The block outlives this call, so the caller's pane array is copied into
  // block-owned storage here and the block reads only that copy.
  NSData *paneData = [NSData dataWithBytes:panes
                                    length:sizeof(*panes) * (size_t)pane_count];
  on_main_async(^{
    const ScreenwideWorkspacePaneRect *copiedPanes = paneData.bytes;
    surface.workspaceMode = YES;
    surface.workspaceLayoutAwaitsPresent = defer_draw != 0;
    BOOL ownsFrame = surface.interaction.selectionDragActive &&
        (surface.interaction.selectionDragOperation == 3 ||
         (surface.interaction.selectionDragOperation == 0 &&
          !NSIsEmptyRect(surface.interaction.selectionMoveFrameStart)));
    BOOL naturalSizeChanged = surface.workspaceNaturalWidth > 0.0 &&
        (fabs(surface.workspaceNaturalWidth - natural_width) > 0.51 ||
         fabs(surface.workspaceNaturalHeight - natural_height) > 0.51);
    if (!ownsFrame && naturalSizeChanged)
      restore_workspace_transform(surface, natural_width, natural_height);
    if (!ownsFrame) surface.keepTransformForCommittedNaturalSize = NO;
    while (surface.views.count == 0)
      [surface.views addObject:make_view(surface)];
    while (surface.editorBaseRects.count < pane_count)
      [surface.editorBaseRects addObject:[NSValue valueWithRect:NSZeroRect]];
    for (uint32_t index = 0; index < pane_count; index++) {
      const ScreenwideWorkspacePaneRect *pane = &copiedPanes[index];
      while (surface.editorBaseRects.count <= pane->index)
        [surface.editorBaseRects addObject:[NSValue valueWithRect:NSZeroRect]];
      surface.editorBaseRects[pane->index] = [NSValue valueWithRect:
          NSMakeRect(pane->x, pane->y, pane->width, pane->height)];
    }
    NSMutableSet<NSNumber *> *active = [NSMutableSet setWithCapacity:pane_count];
    for (uint32_t index = 0; index < pane_count; index++)
      [active addObject:@(copiedPanes[index].index)];
    surface.workspaceActivePaneIndices = active;
    ScreenwidePreviewView *workspace = surface.views[0];
    workspace.frame = surface.container.bounds;
    workspace.active = YES;
    workspace.hidden = NO;
    CGFloat scale = surface.host.window.backingScaleFactor ?: 1.0;
    CAMetalLayer *layer = (CAMetalLayer *)workspace.layer;
    layer.contentsScale = scale;
    layer.presentsWithTransaction = YES;
    layer.drawableSize = CGSizeMake(MAX(surface.container.bounds.size.width * scale, 2.0),
                                    MAX(surface.container.bounds.size.height * scale, 2.0));
    surface.workspaceNaturalWidth = natural_width;
    surface.workspaceNaturalHeight = natural_height;
    (void)x; (void)y; (void)width; (void)height;
  });
}

void screenwide_preview_surface_finish_layout(void *handle) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  on_main_async(^{
    for (ScreenwidePreviewView *view in surface.views) {
      if (view.active) continue;
      view.hidden = YES;
      view.hasPendingFrame = NO;
    }
    if (!(surface.workspaceMode && surface.workspaceLayoutAwaitsPresent))
      redraw_selection(surface);
    surface.workspaceLayoutAwaitsPresent = NO;
    invalidate_selection_cursor_rects(surface);
  });
}
