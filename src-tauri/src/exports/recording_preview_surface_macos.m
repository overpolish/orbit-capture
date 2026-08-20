// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#import <AppKit/AppKit.h>
#import <Metal/Metal.h>
#import <QuartzCore/CAMetalLayer.h>
#import <QuartzCore/CATransaction.h>
#import <WebKit/WebKit.h>
#include <math.h>

#import "cursor_export/gpu_compositor_macos.h"
#import "recording_preview_surface_macos_private.h"


typedef struct {
  double zoom;
  double pan_x;
  double pan_y;
} ScreenwideWorkspaceTransform;

@implementation ScreenwidePreviewView
- (NSView *)hitTest:(NSPoint)point { (void)point; return nil; }
@end

@implementation ScreenwidePreviewSurface
@end


SCREENWIDE_PREVIEW_PRIVATE NSRect editor_frame_with_transform(
    ScreenwidePreviewSurface *surface, NSRect base, double zoom,
    NSPoint pan) {
  double viewportWidth = surface.container.bounds.size.width;
  double viewportHeight = surface.container.bounds.size.height;
  double width = base.size.width * zoom;
  double height = base.size.height * zoom;
  double centerX = viewportWidth / 2.0 + pan.x +
                   (NSMidX(base) - viewportWidth / 2.0) * zoom;
  double centerY = viewportHeight / 2.0 + pan.y +
                   (NSMidY(base) - viewportHeight / 2.0) * zoom;
  return NSMakeRect(centerX - width / 2.0,
                    viewportHeight - centerY - height / 2.0, width, height);
}

SCREENWIDE_PREVIEW_PRIVATE NSRect editor_frame(
    ScreenwidePreviewSurface *surface, NSRect base) {
  return editor_frame_with_transform(
      surface, base, surface.editorZoom,
      NSMakePoint(surface.editorPanX, surface.editorPanY));
}

/// Re-express a resized workspace against its new fit rectangle without
/// changing a single displayed pixel. Frame gestures use their immutable
/// starting transform to produce `displayed`; this function changes only the
/// fit-relative zoom/pan representation used by subsequent gestures and the
/// toolbar.
SCREENWIDE_PREVIEW_PRIVATE NSRect rebase_workspace_fit(ScreenwidePreviewSurface *surface,
                                   NSRect displayed) {
  NSSize viewport = surface.container.bounds.size;
  ScreenwideDisplayRect topLeftDisplayed = {
    displayed.origin.x,
    viewport.height - NSMaxY(displayed),
    displayed.size.width,
    displayed.size.height,
  };
  ScreenwideDisplayFitRebase rebased = screenwide_workspace_rebase_display_fit(
      viewport.width, viewport.height, topLeftDisplayed, 8.0);
  surface.editorZoom = rebased.zoom;
  surface.editorPanX = rebased.pan_x;
  surface.editorPanY = rebased.pan_y;
  return NSMakeRect(rebased.fit.x, rebased.fit.y,
                    rebased.fit.width, rebased.fit.height);
}

static NSString *workspace_size_key(double width, double height) {
  return [NSString stringWithFormat:@"%lldx%lld",
          (long long)llround(width), (long long)llround(height)];
}

SCREENWIDE_PREVIEW_PRIVATE void remember_workspace_transform(
    ScreenwidePreviewSurface *surface, double width, double height) {
  if (width <= 0.0 || height <= 0.0) return;
  ScreenwideWorkspaceTransform transform = {
    surface.editorZoom, surface.editorPanX, surface.editorPanY,
  };
  surface.workspaceTransforms[workspace_size_key(width, height)] =
      [NSValue valueWithBytes:&transform objCType:@encode(ScreenwideWorkspaceTransform)];
}

SCREENWIDE_PREVIEW_PRIVATE void restore_workspace_transform(
    ScreenwidePreviewSurface *surface, double width, double height) {
  // A size the user just produced by dragging is not a history jump: the
  // auto-fit samples rebased zoom/pan so the grown canvas already sits where
  // the pointer left it, and React's integer canvas can differ from the
  // native float estimate by a pixel, which must not recentre the view.
  if (surface.keepTransformForCommittedNaturalSize) {
    surface.keepTransformForCommittedNaturalSize = NO;
    return;
  }
  NSValue *value = surface.workspaceTransforms[workspace_size_key(width, height)];
  if (value == nil) return;
  ScreenwideWorkspaceTransform transform;
  [value getValue:&transform size:sizeof(transform)];
  BOOL zoomChanged = fabs(surface.editorZoom - transform.zoom) > 0.000001;
  // Undo/redo restores the zoom belonging to that frame size, but recenters
  // instead of restoring the workspace's old pan offset.
  surface.editorZoom = transform.zoom;
  surface.editorPanX = 0.0;
  surface.editorPanY = 0.0;
  if (zoomChanged && surface.transformCallback)
    surface.transformCallback(transform.zoom * 100.0,
                              surface.transformContext);
}

SCREENWIDE_PREVIEW_PRIVATE NSRect selection_display_frame_for(
    ScreenwidePreviewSurface *surface,
    ScreenwidePreviewSelection selection);
SCREENWIDE_PREVIEW_PRIVATE BOOL selection_is_frame(ScreenwidePreviewSurface *surface);
SCREENWIDE_PREVIEW_PRIVATE void redraw_workspace(ScreenwidePreviewSurface *surface);
SCREENWIDE_PREVIEW_PRIVATE void update_crop_magnifier(ScreenwidePreviewSurface *surface,
                                  NSPoint point, uint32_t edges);
SCREENWIDE_PREVIEW_PRIVATE void begin_workspace_frame_resize(ScreenwidePreviewSurface *surface);
SCREENWIDE_PREVIEW_PRIVATE void update_workspace_frame_resize(
    ScreenwidePreviewSurface *surface, NSRect start, NSRect resized);
SCREENWIDE_PREVIEW_PRIVATE BOOL update_workspace_auto_fit_move(
    ScreenwidePreviewSurface *surface, uint32_t selected_layer,
    double move_x, double move_y, NSRect start, NSRect resized);
SCREENWIDE_PREVIEW_PRIVATE void end_workspace_frame_resize(
    ScreenwidePreviewSurface *surface, BOOL commit);

SCREENWIDE_PREVIEW_PRIVATE void reflow_recording_workspace_panes(
    ScreenwidePreviewSurface *surface, NSArray<NSValue *> *starts,
    NSUInteger selectedPane, NSRect resized) {
  if (starts.count == 0 || selectedPane >= starts.count) return;
  NSMutableArray<NSNumber *> *order = [NSMutableArray array];
  for (NSUInteger index = 0; index < starts.count; index++)
    if ([surface.workspaceActivePaneIndices containsObject:@(index)])
      [order addObject:@(index)];
  [order sortUsingComparator:^NSComparisonResult(NSNumber *left,
                                                  NSNumber *right) {
    CGFloat leftX = starts[left.unsignedIntegerValue].rectValue.origin.x;
    CGFloat rightX = starts[right.unsignedIntegerValue].rectValue.origin.x;
    return leftX < rightX ? NSOrderedAscending
         : leftX > rightX ? NSOrderedDescending : NSOrderedSame;
  }];
  NSUInteger selectedOrder = [order indexOfObject:@(selectedPane)];
  if (selectedOrder == NSNotFound) return;
  NSMutableArray<NSValue *> *next = [starts mutableCopy];
  next[selectedPane] = [NSValue valueWithRect:resized];
  CGFloat maximumHeight = 0.0;
  for (NSNumber *value in order)
    maximumHeight = MAX(maximumHeight,
                        next[value.unsignedIntegerValue].rectValue.size.height);
  CGFloat groupTop = resized.origin.y -
      (maximumHeight - resized.size.height) / 2.0;
  for (NSNumber *value in order) {
    NSUInteger index = value.unsignedIntegerValue;
    NSRect frame = next[index].rectValue;
    frame.origin.y = groupTop + (maximumHeight - frame.size.height) / 2.0;
    next[index] = [NSValue valueWithRect:frame];
  }
  for (NSUInteger position = selectedOrder + 1; position < order.count;
       position++) {
    NSUInteger previous = order[position - 1].unsignedIntegerValue;
    NSUInteger index = order[position].unsignedIntegerValue;
    NSRect previousStart = starts[previous].rectValue;
    NSRect start = starts[index].rectValue;
    CGFloat gap = NSMinX(start) - NSMaxX(previousStart);
    NSRect frame = next[index].rectValue;
    frame.origin.x = NSMaxX(next[previous].rectValue) + gap;
    next[index] = [NSValue valueWithRect:frame];
  }
  for (NSInteger position = (NSInteger)selectedOrder - 1; position >= 0;
       position--) {
    NSUInteger index = order[(NSUInteger)position].unsignedIntegerValue;
    NSUInteger following = order[(NSUInteger)position + 1].unsignedIntegerValue;
    NSRect start = starts[index].rectValue;
    NSRect followingStart = starts[following].rectValue;
    CGFloat gap = NSMinX(followingStart) - NSMaxX(start);
    NSRect frame = next[index].rectValue;
    frame.origin.x = NSMinX(next[following].rectValue) - gap - frame.size.width;
    next[index] = [NSValue valueWithRect:frame];
  }
  surface.editorBaseRects = next;
}

SCREENWIDE_PREVIEW_PRIVATE void rebase_recording_workspace_fit(
    ScreenwidePreviewSurface *surface, NSArray<NSValue *> *starts,
    double zoom, NSPoint pan) {
  NSRect bounds = NSZeroRect;
  NSRect startBounds = NSZeroRect;
  NSRect displayed = NSZeroRect;
  BOOL hasBounds = NO;
  for (NSNumber *value in surface.workspaceActivePaneIndices) {
    NSUInteger index = value.unsignedIntegerValue;
    if (index >= surface.editorBaseRects.count) continue;
    NSRect frame = surface.editorBaseRects[index].rectValue;
    NSRect start = index < starts.count ? starts[index].rectValue : frame;
    NSRect shown = editor_frame_with_transform(surface, frame, zoom, pan);
    bounds = hasBounds ? NSUnionRect(bounds, frame) : frame;
    startBounds = hasBounds ? NSUnionRect(startBounds, start) : start;
    displayed = hasBounds ? NSUnionRect(displayed, shown) : shown;
    hasBounds = YES;
  }
  if (!hasBounds || NSIsEmptyRect(bounds) || NSIsEmptyRect(displayed)) return;
  surface.workspaceNaturalWidth = surface.workspaceResizeNaturalWidth *
      bounds.size.width / MAX(startBounds.size.width, 1.0);
  surface.workspaceNaturalHeight = surface.workspaceResizeNaturalHeight *
      bounds.size.height / MAX(startBounds.size.height, 1.0);
  NSRect fit = rebase_workspace_fit(surface, displayed);
  double scaleX = fit.size.width / MAX(bounds.size.width, 1.0);
  double scaleY = fit.size.height / MAX(bounds.size.height, 1.0);
  NSMutableArray<NSValue *> *rebased = [surface.editorBaseRects mutableCopy];
  for (NSNumber *value in surface.workspaceActivePaneIndices) {
    NSUInteger index = value.unsignedIntegerValue;
    if (index >= rebased.count) continue;
    NSRect frame = surface.editorBaseRects[index].rectValue;
    frame = NSMakeRect(
        fit.origin.x + (frame.origin.x - bounds.origin.x) * scaleX,
        fit.origin.y + (frame.origin.y - bounds.origin.y) * scaleY,
        frame.size.width * scaleX,
        frame.size.height * scaleY);
    rebased[index] = [NSValue valueWithRect:frame];
  }
  surface.editorBaseRects = rebased;
}

SCREENWIDE_PREVIEW_PRIVATE void redraw_selection(
    ScreenwidePreviewSurface *surface) {
  [surface redrawSelection];
}

SCREENWIDE_PREVIEW_PRIVATE void apply_editor_transform(ScreenwidePreviewSurface *surface) {
  if (!surface.editorEnabled) return;
  if (surface.workspaceMode) {
    if (surface.views.count > 0) {
      ScreenwidePreviewView *workspace = surface.views[0];
      workspace.frame = surface.container.bounds;
      workspace.hasPendingFrame = NO;
    }
    redraw_workspace(surface);
    invalidate_selection_cursor_rects(surface);
    return;
  }
  for (NSUInteger index = 0; index < surface.views.count; index++) {
    ScreenwidePreviewView *view = surface.views[index];
    if (!view.active || index >= surface.editorBaseRects.count) continue;
    view.frame = editor_frame(surface, surface.editorBaseRects[index].rectValue);
    view.hasPendingFrame = NO;
  }
  redraw_selection(surface);
  invalidate_selection_cursor_rects(surface);
}


SCREENWIDE_PREVIEW_PRIVATE void set_editor_zoom(ScreenwidePreviewSurface *surface,
                            double zoom, NSPoint anchor) {
  double previous = surface.editorZoom;
  zoom = fmin(16.0, fmax(0.1, zoom));
  if (fabs(previous - zoom) < 0.000001) return;
  double centeredX = anchor.x - NSMidX(surface.interaction.bounds);
  double centeredY = anchor.y - NSMidY(surface.interaction.bounds);
  double ratio = zoom / previous;
  surface.editorPanX =
      centeredX - (centeredX - surface.editorPanX) * ratio;
  surface.editorPanY =
      centeredY - (centeredY - surface.editorPanY) * ratio;
  surface.editorZoom = zoom;
  apply_editor_transform(surface);
  if (surface.transformCallback)
    surface.transformCallback(zoom * 100.0, surface.transformContext);
}

@implementation ScreenwidePreviewInteractionView
@end

static NSString *const shader = @R"(
#include <metal_stdlib>
using namespace metal;
kernel void present_rgba(const device uchar4 *source [[buffer(0)]],
                         constant uint2 &content [[buffer(1)]],
                         texture2d<float, access::write> output [[texture(0)]],
                         uint2 gid [[thread_position_in_grid]]) {
  if (gid.x >= output.get_width() || gid.y >= output.get_height()) return;
  if (gid.x >= content.x || gid.y >= content.y) {
    output.write(float4(0.0), gid);
    return;
  }
  uchar4 pixel = source[gid.y * content.x + gid.x];
  output.write(float4(pixel.r, pixel.g, pixel.b, pixel.a) / 255.0, gid);
}

struct selection_vertex {
  float2 position;
  float2 uv;
  uint kind;
  uint padding;
};

struct selection_out {
  float4 position [[position]];
  float2 uv;
  uint kind;
};

vertex selection_out selection_vertex_main(const device selection_vertex *vertices [[buffer(0)]],
                                           uint index [[vertex_id]]) {
  selection_out out;
  out.position = float4(vertices[index].position, 0.0, 1.0);
  out.uv = vertices[index].uv;
  out.kind = vertices[index].kind;
  return out;
}

fragment float4 selection_fragment(selection_out in [[stage_in]],
                                   constant uint &light_mode [[buffer(0)]],
                                   constant float4 &magnifier_box [[buffer(1)]],
                                   texture2d<float> label [[texture(0)]]) {
  constexpr sampler label_sampler(filter::linear, address::clamp_to_edge);
  if (magnifier_box.z > 0.0) {
    float2 half_size = magnifier_box.zw * 0.5;
    float2 local = abs(in.position.xy - (magnifier_box.xy + half_size)) -
                   (half_size - 4.0);
    float distance = length(max(local, 0.0)) +
                     min(max(local.x, local.y), 0.0) - 4.0;
    if (distance <= 0.0) discard_fragment();
  }
  if (in.kind == 11) {
    // The label bitmap is premultiplied, but this pipeline blends with
    // SourceAlpha/OneMinusSourceAlpha (i.e. it expects straight alpha), so the
    // colour is un-premultiplied back out before it is returned.
    float4 sampled = label.sample(label_sampler, in.uv);
    if (sampled.a <= 0.002) discard_fragment();
    return float4(sampled.rgb / sampled.a, sampled.a);
  }
  if (in.kind == 6) return float4(0.0, 0.0, 0.0, 0.4);
  if (in.kind >= 7 && in.kind <= 10) {
    bool horizontal = in.kind <= 8;
    bool halo = in.kind == 8 || in.kind == 10;
    float coordinate = horizontal ? in.uv.x : in.uv.y;
    float wave = abs(fract(coordinate) - 0.5);
    float aa = max(fwidth(wave), 0.001);
    float coverage = 1.0 - smoothstep(0.30, 0.30 + aa, wave);
    if (coverage <= 0.0) discard_fragment();
    float4 color = light_mode != 0
        ? (halo ? float4(1.0) : float4(0.12, 0.12, 0.12, 1.0))
        : (halo ? float4(0.0, 0.0, 0.0, 0.8) : float4(1.0));
    color.a *= coverage;
    return color;
  }
  float coverage = 1.0;
  bool guide = in.kind == 4 || in.kind == 5;
  if (!guide && (in.kind & 1) != 0) {
    float edge = distance(in.uv, float2(0.5));
    float aa = max(fwidth(edge), 0.001);
    // Keep the fill fully opaque and spend the AA ramp outside its edge. This
    // preserves the same perceived colour as the pixel-snapped line core.
    coverage = 1.0 - smoothstep(0.5, 0.5 + aa, edge);
    if (coverage <= 0.0) discard_fragment();
  }
  if (guide) {
    if (in.kind == 5)
      return light_mode != 0 ? float4(0.008, 0.518, 0.780, 1.0)
                             : float4(0.055, 0.647, 0.914, 1.0);
    return float4(0.918, 0.702, 0.031, 1.0);
  }
  bool halo = in.kind >= 2;
  float4 color = light_mode != 0
      ? (halo ? float4(1.0) : float4(0.12, 0.12, 0.12, 1.0))
      : (halo ? float4(0.0, 0.0, 0.0, 0.8) : float4(1.0));
  color.a *= coverage;
  return color;
}
)";

static void on_main(dispatch_block_t block) {
  if ([NSThread isMainThread]) block();
  else dispatch_sync(dispatch_get_main_queue(), block);
}

/// Runs `block` on the main thread without ever blocking the caller. Every
/// void setter below uses this instead of `on_main`: the layout command runs
/// on Tauri's async pool while it holds the preview player mutex, and the
/// main thread takes that same mutex from the sync seek command. A
/// `dispatch_sync` there deadlocks the app - main waits for the mutex, the
/// pool thread waits for the main queue that only main can drain. The main
/// queue is serial, so the setters still apply in call order; a caller
/// already on the main thread runs inline and keeps today's exact behaviour
/// (the layout/present sequences that share one transaction).
SCREENWIDE_PREVIEW_PRIVATE void on_main_async(dispatch_block_t block) {
  if ([NSThread isMainThread]) block();
  else dispatch_async(dispatch_get_main_queue(), block);
}

/// Runs `body` on the main thread inside an explicit Core Animation
/// transaction. From a background thread it is dispatched asynchronously: the
/// decoder thread is joined from the main thread on stop, so it must never
/// block on the main queue. On the main thread it runs inline so a caller
/// that lays out right after (the screenshot preview) shares the transaction.
static void run_on_main_transaction(dispatch_block_t body) {
  dispatch_block_t block = ^{
    [CATransaction begin];
    [CATransaction setDisableActions:YES];
    body();
    [CATransaction commit];
  };
  if ([NSThread isMainThread]) block();
  else dispatch_async(dispatch_get_main_queue(), block);
}

/// Main thread only. Applies every pane's pending frame and presents the given
/// drawables inside the caller's transaction.
static void commit_frames_and_drawables(ScreenwidePreviewSurface *surface,
                                        NSArray<id<CAMetalDrawable>> *drawables,
                                        NSArray<ScreenwidePreviewView *> *views) {
  for (ScreenwidePreviewView *view in surface.views) {
    if (!view.hasPendingFrame) continue;
    view.frame = view.pendingFrame;
    view.hasPendingFrame = NO;
  }
  for (id<CAMetalDrawable> drawable in drawables) [drawable present];
  for (ScreenwidePreviewView *view in views) {
    if (!view.active) continue;
    surface.container.hidden = NO;
    view.hidden = NO;
  }
}

/// Commits `command` and presents `drawable` together with every pending pane
/// frame in one transaction (or hands it to the open batch, which does the
/// same for all panes at `end_present`). `presentsWithTransaction` requires
/// the command buffer to be scheduled before `present`; unbatched presents (and
/// batched ones encoded on the main thread, which must present in the acquiring
/// turn - see below) wait for that here on the calling thread, while off-main
/// batched ones inherit it from waiting on full GPU completion (completed
/// implies scheduled).
SCREENWIDE_PREVIEW_PRIVATE void present_in_transaction(
    ScreenwidePreviewSurface *surface, ScreenwidePreviewView *view,
    id<MTLCommandBuffer> command, id<CAMetalDrawable> drawable) {
  // ORDERING: Metal only accepts completed handlers before `commit`, so the
  // batch membership decision (which needs a handler) has to happen first -
  // hence the lock/handler/commit order rather than the older commit-first one.
  [surface.batchLock lock];
  if (surface.batchDepth > 0) {
    if ([NSThread isMainThread]) {
      [surface.batchLock unlock];
      // SAME-TURN CONSTRAINT: this drawable was acquired (`nextDrawable`) on the
      // main thread, so the current runloop turn owns it. A main-thread turn
      // that ends still holding an acquired-but-unpresented drawable of a
      // `presentsWithTransaction` layer makes the turn's closing Core Animation
      // flush wait for that drawable's present - and any present deferred to a
      // later main-queue block (the batch's `dispatch_group_notify`, a completed
      // handler's `dispatch_async`) is queued BEHIND that very flush, so it can
      // never arrive and the flush burns its ~1s watchdog. Measured directly:
      // `screenwide-present-batch-enter: 1005 ms` with an instant commit.
      // Presents from main-thread encodes therefore MUST land in the acquiring
      // turn; batching them buys nothing here anyway (both batched drawables are
      // the same pane-sized workspace layer, so there is no cross-layer
      // atomicity at stake).
      [command commit];
      [command waitUntilScheduled];
      [CATransaction begin];
      [CATransaction setDisableActions:YES];
      commit_frames_and_drawables(surface, @[drawable], @[view]);
      [CATransaction commit];
      return;
    }
    // Off-main acquisition: no main-thread flush is holding this drawable
    // hostage, so deferring the present to `end_present` is safe and keeps the
    // GPU wait off the main thread.
    // Capture the group in a local: the property is replaced by the next
    // `begin_present`, and this handler must leave the group it entered.
    dispatch_group_t group = surface.batchGroup;
    if (group != nil) {
      dispatch_group_enter(group);
      [command addCompletedHandler:^(__unused id<MTLCommandBuffer> completed) {
        dispatch_group_leave(group);
      }];
    }
    [surface.batchDrawables addObject:drawable];
    [surface.batchViews addObject:view];
    [surface.batchLock unlock];
    // No `waitUntilScheduled`: `end_present` now waits for this buffer to
    // complete, which is strictly stronger than scheduled.
    [command commit];
    return;
  }
  [surface.batchLock unlock];
  [command commit];
  [command waitUntilScheduled];
  run_on_main_transaction(^{
    commit_frames_and_drawables(surface, @[drawable], @[view]);
  });
}

void screenwide_preview_surface_begin_present(void *handle) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  [surface.batchLock lock];
  surface.batchDepth += 1;
  // Only the outermost begin opens a group; nested begins join the open one.
  if (surface.batchDepth == 1) surface.batchGroup = dispatch_group_create();
  [surface.batchLock unlock];
}

/// Closes a batch. Runs even when nothing was presented so a deferred layout
/// whose composition failed still lands instead of leaving the panes stuck.
void screenwide_preview_surface_end_present(void *handle) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  [surface.batchLock lock];
  surface.batchDepth = MAX(surface.batchDepth - 1, 0);
  if (surface.batchDepth > 0) {
    [surface.batchLock unlock];
    return;
  }
  NSArray<id<CAMetalDrawable>> *drawables = [surface.batchDrawables copy];
  NSArray<ScreenwidePreviewView *> *views = [surface.batchViews copy];
  dispatch_group_t group = surface.batchGroup;
  [surface.batchDrawables removeAllObjects];
  [surface.batchViews removeAllObjects];
  // The group property stays until the next `begin_present` replaces it; this
  // snapshot owns it from here on.
  [surface.batchLock unlock];
  if (drawables.count == 0 || group == nil) {
    // Nothing composed: flush the pending layout immediately, there is no GPU
    // work to wait for.
    run_on_main_transaction(^{
      commit_frames_and_drawables(surface, drawables, views);
    });
    return;
  }
  // Only OFF-MAIN acquisitions ever reach here: `present_in_transaction`
  // presents main-thread-encoded drawables inline in their acquiring turn,
  // because a deferred present would be queued behind that turn's own Core
  // Animation flush (which blocks on it) and hang for the flush's ~1s watchdog.
  // For off-main acquisitions there is no such flush to trap, so the present
  // can wait for every batched command buffer to COMPLETE. That matters because
  // a `presentsWithTransaction` layer holds the Core Animation commit open
  // until its drawable's GPU work finishes, and that commit publishes the WHOLE
  // window's layer tree (webview UI, OSC included) - so committing while a
  // paused full-resolution still (~8MP compute dispatch) is still running froze
  // all painting for most of a second. By notify time the work is done, so the
  // commit only waits on the WindowServer handshake. Notify targets the main
  // queue directly: an intermediate concurrent-queue hop could let successive
  // batches' presents land out of GPU-completion order.
  dispatch_group_notify(group, dispatch_get_main_queue(), ^{
    [CATransaction begin];
    [CATransaction setDisableActions:YES];
    commit_frames_and_drawables(surface, drawables, views);
    [CATransaction commit];
  });
}

void *screenwide_preview_surface_create(void *host_view) {
  if (host_view == NULL) return NULL;
  __block ScreenwidePreviewSurface *surface;
  on_main(^{
    surface = [ScreenwidePreviewSurface new];
    surface.host = (__bridge NSView *)host_view;
    surface.device = MTLCreateSystemDefaultDevice();
    surface.queue = [surface.device newCommandQueue];
    NSError *error = nil;
    id<MTLLibrary> library = [surface.device newLibraryWithSource:shader options:nil error:&error];
    surface.pipeline = [surface.device newComputePipelineStateWithFunction:
      [library newFunctionWithName:@"present_rgba"] error:&error];
    MTLRenderPipelineDescriptor *selectionDescriptor = [MTLRenderPipelineDescriptor new];
    selectionDescriptor.vertexFunction = [library newFunctionWithName:@"selection_vertex_main"];
    selectionDescriptor.fragmentFunction = [library newFunctionWithName:@"selection_fragment"];
    selectionDescriptor.colorAttachments[0].pixelFormat = MTLPixelFormatBGRA8Unorm;
    selectionDescriptor.colorAttachments[0].blendingEnabled = YES;
    selectionDescriptor.colorAttachments[0].sourceRGBBlendFactor = MTLBlendFactorSourceAlpha;
    selectionDescriptor.colorAttachments[0].destinationRGBBlendFactor = MTLBlendFactorOneMinusSourceAlpha;
    selectionDescriptor.colorAttachments[0].sourceAlphaBlendFactor = MTLBlendFactorSourceAlpha;
    selectionDescriptor.colorAttachments[0].destinationAlphaBlendFactor = MTLBlendFactorOneMinusSourceAlpha;
    surface.selectionPipeline = [surface.device newRenderPipelineStateWithDescriptor:selectionDescriptor
                                                                                  error:&error];
    // A 1x1 transparent texture stands in whenever no size readout exists, so
    // the fragment function's texture slot is always bound (see
    // `selectionLabelPlaceholder`).
    MTLTextureDescriptor *placeholderDescriptor = [MTLTextureDescriptor
        texture2DDescriptorWithPixelFormat:MTLPixelFormatRGBA8Unorm
                                     width:1
                                    height:1
                                 mipmapped:NO];
    placeholderDescriptor.usage = MTLTextureUsageShaderRead;
    surface.selectionLabelPlaceholder =
        [surface.device newTextureWithDescriptor:placeholderDescriptor];
    const uint8_t transparent[4] = {0, 0, 0, 0};
    [surface.selectionLabelPlaceholder replaceRegion:MTLRegionMake2D(0, 0, 1, 1)
                                         mipmapLevel:0
                                           withBytes:transparent
                                         bytesPerRow:4];
    surface.container = [[ScreenwidePreviewView alloc] initWithFrame:NSZeroRect];
    surface.container.wantsLayer = YES;
    surface.container.layer.masksToBounds = YES;
    surface.container.hidden = YES;
    // The panes live directly BELOW the webview: the DOM keeps every control
    // and just mask-punches holes over the pane rects, exactly like FCP
    // layers its on-screen controls above the video surface. The container
    // must sit immediately under the WKWebView specifically - dropping it to
    // the bottom of the window would put it beneath the vibrancy effect view,
    // which shows the video through frosted glass.
    NSView *webview = nil;
    for (NSView *subview in surface.host.subviews) {
      if ([subview isKindOfClass:[WKWebView class]]) {
        webview = subview;
        break;
      }
    }
    if (webview != nil) {
      [surface.host addSubview:surface.container
                    positioned:NSWindowBelow
                    relativeTo:webview];
    } else if ([surface.host isKindOfClass:[WKWebView class]] &&
               surface.host.superview != nil) {
      // The handle is the webview itself: become its sibling, directly below.
      [surface.host.superview addSubview:surface.container
                              positioned:NSWindowBelow
                              relativeTo:surface.host];
    } else {
      [surface.host addSubview:surface.container positioned:NSWindowAbove relativeTo:nil];
    }
    surface.webview = webview != nil
                          ? webview
                          : ([surface.host isKindOfClass:[WKWebView class]]
                                 ? surface.host
                                 : nil);
    surface.interaction = [[ScreenwidePreviewInteractionView alloc] initWithFrame:NSZeroRect];
    surface.interaction.surface = surface;
    surface.interaction.wantsLayer = YES;
    surface.interaction.layer.masksToBounds = YES;
    surface.selectionLayer = [CAMetalLayer layer];
    surface.selectionLayer.device = surface.device;
    surface.selectionLayer.pixelFormat = MTLPixelFormatBGRA8Unorm;
    surface.selectionLayer.framebufferOnly = YES;
    surface.selectionLayer.opaque = NO;
    surface.selectionLayer.presentsWithTransaction = NO;
    // This manually managed sublayer begins at zero size. Without explicit
    // actions, its first bounds/position assignment animates from the origin,
    // making the OSC appear to scale in when the export window opens.
    NSNull *noAction = [NSNull null];
    surface.selectionLayer.actions = @{
      @"bounds": noAction,
      @"position": noAction,
      @"hidden": noAction,
      @"opacity": noAction,
      @"contents": noAction,
    };
    [surface.interaction.layer addSublayer:surface.selectionLayer];
    surface.selectionLayer.hidden = YES;
    surface.interaction.hidden = YES;
    if (webview != nil) {
      [surface.host addSubview:surface.interaction
                    positioned:NSWindowAbove
                    relativeTo:webview];
    } else if ([surface.host isKindOfClass:[WKWebView class]] &&
               surface.host.superview != nil) {
      [surface.host.superview addSubview:surface.interaction
                              positioned:NSWindowAbove
                              relativeTo:surface.host];
    } else {
      [surface.host addSubview:surface.interaction positioned:NSWindowAbove relativeTo:nil];
    }
    surface.editorZoom = 1.0;
    surface.selectionVisible = YES;
    surface.editorBaseRects = [NSMutableArray array];
    surface.views = [NSMutableArray array];
    surface.batchLock = [NSLock new];
    surface.workspaceLock = [NSLock new];
    surface.workspaceTransforms = [NSMutableDictionary dictionary];
    surface.batchDrawables = [NSMutableArray array];
    surface.batchViews = [NSMutableArray array];
    install_native_cursor_guard();
  });
  if (surface.pipeline == nil) return NULL;
  return (__bridge_retained void *)surface;
}

int screenwide_preview_surface_present(void *handle, uint32_t index,
                                  const uint8_t *rgba, uint32_t width, uint32_t height) {
  if (handle == NULL || rgba == NULL || width == 0 || height == 0) return 0;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  // Layout is driven by the webview and may arrive one display tick after
  // playback. Dropping that frame is harmless; ending playback is not.
  if (index >= surface.views.count) return 1;
  ScreenwidePreviewView *view = surface.views[index];
  if (!view.active) return 1;
  CAMetalLayer *layer = (CAMetalLayer *)view.layer;
  layer.drawableSize = CGSizeMake(width, height);
  id<CAMetalDrawable> drawable = [layer nextDrawable];
  if (drawable == nil) return 0;
  NSUInteger length = (NSUInteger)width * height * 4;
  id<MTLBuffer> pixels = [surface.device newBufferWithBytes:rgba length:length
                                                    options:MTLResourceStorageModeShared];
  id<MTLCommandBuffer> command = [surface.queue commandBuffer];
  id<MTLComputeCommandEncoder> encoder = [command computeCommandEncoder];
  [encoder setComputePipelineState:surface.pipeline];
  [encoder setBuffer:pixels offset:0 atIndex:0];
  uint32_t content[2] = {width, height};
  [encoder setBytes:content length:sizeof(content) atIndex:1];
  [encoder setTexture:drawable.texture atIndex:0];
  NSUInteger drawable_width = drawable.texture.width;
  NSUInteger drawable_height = drawable.texture.height;
  NSUInteger group_width = MIN(surface.pipeline.threadExecutionWidth, drawable_width);
  NSUInteger group_height = MIN(MAX((NSUInteger)1,
    surface.pipeline.maxTotalThreadsPerThreadgroup / MAX(group_width, (NSUInteger)1)),
    drawable_height);
  [encoder dispatchThreads:MTLSizeMake(drawable_width, drawable_height, 1)
       threadsPerThreadgroup:MTLSizeMake(group_width, group_height, 1)];
  [encoder endEncoding];
  present_in_transaction(surface, view, command, drawable);
  return 1;
}

void screenwide_preview_surface_hide(void *handle) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  dispatch_async(dispatch_get_main_queue(), ^{
    [surface.interaction releaseCursorControl];
    surface.container.hidden = YES;
    surface.interaction.hidden = YES;
    for (ScreenwidePreviewView *view in surface.views) view.hidden = YES;
  });
}

/// Releases a caller-owned callback context behind every block already queued
/// on the main queue. The callback setters install and clear their context
/// asynchronously, so a caller that clears a callback and frees its context
/// straight away would pull the memory out from under a main-thread gesture
/// that is still holding the old pointer. Handing the free to the main queue
/// orders it after the clear, which is the last block that can read it.
void screenwide_preview_surface_release_context_on_main(
    void (*release)(void *), void *context) {
  if (release == NULL) return;
  dispatch_async(dispatch_get_main_queue(), ^{ release(context); });
}

void screenwide_preview_surface_destroy(void *handle) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge_transfer ScreenwidePreviewSurface *)handle;
  dispatch_async(dispatch_get_main_queue(), ^{
    for (ScreenwidePreviewView *view in surface.views) {
      screenwide_gpu_still_presenter_destroy(view.compositor);
      view.compositor = NULL;
    }
    [surface.container removeFromSuperview];
    [surface.interaction removeFromSuperview];
  });
}
