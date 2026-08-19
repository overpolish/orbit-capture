// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#import <AppKit/AppKit.h>
#import <Metal/Metal.h>
#import <QuartzCore/CAMetalLayer.h>
#import <QuartzCore/CATransaction.h>
#import <WebKit/WebKit.h>
#import <objc/runtime.h>
#include <math.h>

#import "cursor_export/gpu_compositor_macos.h"

static const uint32_t ScreenwideFrameLayerId = UINT32_MAX;
static const uint32_t ScreenwideCenteredResizeEdge = 1u << 16;

typedef void (*screenwide_preview_transform_callback)(double zoom_percent,
                                                       void *context);
typedef void (*screenwide_preview_selection_gesture_callback)(uint32_t phase,
                                                               uint32_t pane_index,
                                                               uint32_t operation,
                                                               uint32_t edges,
                                                               double scale,
                                                               double delta_x,
                                                               double delta_y,
                                                               void *context);
typedef void (*screenwide_preview_selection_callback)(int32_t pane_index,
                                                       void *context);

typedef struct {
  uint32_t pane_index;
  uint32_t layer_id;
  uint32_t crop_mode;
  uint32_t radius_disabled;
  double x;
  double y;
  double width;
  double height;
  double radius_percent;
  double image_x;
  double image_y;
  double image_width;
  double image_height;
} ScreenwidePreviewSelection;

typedef struct {
  double x;
  double y;
  double width;
  double height;
} ScreenwideDisplayRect;

typedef struct {
  uint64_t id;
  ScreenwideDisplayRect rect;
  uint8_t radius_enabled;
  double radius_percent;
  int32_t z_order;
  uint8_t selected;
  uint8_t visible;
} ScreenwideDisplayTarget;

typedef struct {
  uint8_t found;
  uint64_t target_id;
  uint8_t handle;
} ScreenwideDisplayHit;

typedef struct {
  ScreenwideDisplayRect fit;
  double zoom;
  double pan_x;
  double pan_y;
} ScreenwideDisplayFitRebase;

extern ScreenwideDisplayHit screenwide_workspace_hit_test(
    const ScreenwideDisplayTarget *targets, size_t count, double x, double y,
    double handle_size);
extern ScreenwideDisplayFitRebase screenwide_workspace_rebase_display_fit(
    double viewport_width, double viewport_height,
    ScreenwideDisplayRect displayed, double gutter);
_Static_assert(sizeof(ScreenwideDisplayTarget) == 64,
               "Rust/C display target layout mismatch");
_Static_assert(sizeof(ScreenwideDisplayHit) == 24,
               "Rust/C display hit layout mismatch");
_Static_assert(sizeof(ScreenwideDisplayFitRebase) == 56,
               "Rust/C display fit rebase layout mismatch");

typedef struct {
  double zoom;
  double pan_x;
  double pan_y;
} ScreenwideWorkspaceTransform;

@class ScreenwidePreviewSurface;

@interface ScreenwidePreviewInteractionView : NSView
@property(nonatomic, weak) ScreenwidePreviewSurface *surface;
@property(nonatomic) NSPoint dragOrigin;
@property(nonatomic) NSPoint dragPan;
@property(nonatomic) NSPoint selectionDragOrigin;
@property(nonatomic) NSRect selectionFrameDragStart;
@property(nonatomic, strong) NSArray<NSValue *> *selectionFramePaneStarts;
@property(nonatomic) double selectionFrameZoomStart;
@property(nonatomic) NSPoint selectionFramePanStart;
@property(nonatomic) NSRect selectionMoveFrameStart;
@property(nonatomic) NSPoint selectionMovePanStart;
@property(nonatomic) double selectionMoveZoomStart;
@property(nonatomic) double selectionMoveDeltaX;
@property(nonatomic) double selectionMoveDeltaY;
@property(nonatomic) BOOL selectionMoveAutoFitActive;
/// The bounds (in mouse-down canvas units) the last auto-fit sample grew the
/// canvas to, so an Option release can re-express the move's starts in the
/// committed canvas and let Option grow it again from there.
@property(nonatomic) NSRect selectionMoveAutoFitBounds;
@property(nonatomic, strong) NSArray<NSValue *> *selectionMoveTargetsStart;
@property(nonatomic) ScreenwidePreviewSelection selectionDragStart;
@property(nonatomic) BOOL selectionDragActive;
@property(nonatomic) BOOL selectionDragCentered;
@property(nonatomic) uint32_t selectionDragOperation;
@property(nonatomic) uint32_t selectionDragEdges;
@property(nonatomic, strong) NSTrackingArea *selectionTrackingArea;
@property(nonatomic) BOOL cursorRectsDisabled;
@property(nonatomic) BOOL panning;
@end

@interface ScreenwidePreviewView : NSView
@property(nonatomic) BOOL active;
@property(nonatomic) void *compositor;
/// A resize the webview has laid out but whose matching frame has not been
/// composed yet. Applying it early would show the previous drawable fitted
/// into the new rect for a display tick; the next present applies it in the
/// same Core Animation transaction as the new pixels.
@property(nonatomic) BOOL hasPendingFrame;
@property(nonatomic) NSRect pendingFrame;
@end

@implementation ScreenwidePreviewView
- (NSView *)hitTest:(NSPoint)point { (void)point; return nil; }
@end

@interface ScreenwidePreviewSurface : NSObject
@property(nonatomic, strong) id<MTLDevice> device;
@property(nonatomic, strong) id<MTLCommandQueue> queue;
@property(nonatomic, strong) id<MTLComputePipelineState> pipeline;
@property(nonatomic, strong) id<MTLRenderPipelineState> selectionPipeline;
@property(nonatomic, strong) NSView *host;
@property(nonatomic, weak) NSView *webview;
@property(nonatomic, strong) ScreenwidePreviewView *container;
@property(nonatomic, strong) NSMutableArray<ScreenwidePreviewView *> *views;
@property(nonatomic) BOOL workspaceMode;
@property(nonatomic) uint32_t workspaceLayerCount;
@property(nonatomic) BOOL workspaceExplicitPlacements;
@property(nonatomic, strong) NSMutableData *workspacePlacements;
@property(nonatomic, strong) NSArray<NSNumber *> *workspacePaneIndices;
@property(nonatomic, strong) NSSet<NSNumber *> *workspaceActivePaneIndices;
@property(nonatomic) double workspaceNaturalWidth;
@property(nonatomic) double workspaceNaturalHeight;
@property(nonatomic) double workspaceResizeNaturalWidth;
@property(nonatomic) double workspaceResizeNaturalHeight;
@property(nonatomic, strong) NSMutableDictionary<NSString *, NSValue *> *workspaceTransforms;
@property(nonatomic) BOOL workspaceDrawInFlight;
@property(nonatomic) BOOL workspaceDrawPending;
@property(nonatomic) BOOL workspaceLayoutAwaitsPresent;
@property(nonatomic, strong) NSLock *workspaceLock;
@property(nonatomic, strong) id<MTLCommandBuffer> workspaceEncodingCommand;
@property(nonatomic, strong) id<MTLTexture> workspaceEncodingTexture;
@property(nonatomic) ScreenwideWorkspaceMagnifier workspaceMagnifier;
@property(nonatomic, strong) ScreenwidePreviewInteractionView *interaction;
@property(nonatomic) BOOL editorEnabled;
@property(nonatomic, strong) NSMutableArray<NSValue *> *editorBaseRects;
@property(nonatomic) double editorPanX;
@property(nonatomic) double editorPanY;
@property(nonatomic) double editorZoom;
@property(nonatomic) screenwide_preview_transform_callback transformCallback;
@property(nonatomic) void *transformContext;
@property(nonatomic) screenwide_preview_selection_gesture_callback selectionGestureCallback;
@property(nonatomic) void *selectionGestureContext;
@property(nonatomic) screenwide_preview_selection_callback selectionCallback;
@property(nonatomic) void *selectionContext;
@property(nonatomic) BOOL selectionHitTestingEnabled;
@property(nonatomic, strong) NSArray<NSValue *> *selectionTargets;
@property(nonatomic) BOOL selectionSnappingEnabled;
@property(nonatomic) BOOL hasSelectionSnapGuideX;
@property(nonatomic) BOOL hasSelectionSnapGuideY;
@property(nonatomic) BOOL selectionSnapGuideXIsObject;
@property(nonatomic) BOOL selectionSnapGuideYIsObject;
@property(nonatomic) double selectionSnapGuideX;
@property(nonatomic) double selectionSnapGuideY;
@property(nonatomic) BOOL hasSelection;
@property(nonatomic) BOOL selectionVisible;
@property(nonatomic) ScreenwidePreviewSelection selection;
@property(nonatomic, strong) CAMetalLayer *selectionLayer;
@property(nonatomic) uint64_t selectionDrawRevision;
@property(nonatomic) BOOL selectionDrawInFlight;
@property(nonatomic) BOOL selectionDrawPending;
/// An open present batch: drawables collected between `begin_present` and
/// `end_present` are presented in ONE Core Animation transaction together
/// with every pane's pending frame, so screen, camera and their new rects
/// change on the same display tick.
@property(nonatomic, strong) NSLock *batchLock;
@property(nonatomic) NSInteger batchDepth;
@property(nonatomic, strong) NSMutableArray<id<CAMetalDrawable>> *batchDrawables;
@property(nonatomic, strong) NSMutableArray<ScreenwidePreviewView *> *batchViews;
/// Tracks the batch's in-flight command buffers: entered per batched present,
/// left from that command buffer's completed handler. `end_present` defers its
/// presenting transaction until the group is empty so the commit never fences
/// on GPU work (see `screenwide_preview_surface_end_present`).
@property(nonatomic, strong) dispatch_group_t batchGroup;
@end

@implementation ScreenwidePreviewSurface
@end

static IMP original_cursor_set = NULL;
static NSCursor *expected_selection_cursor = nil;
static NSCursor *webkit_selection_move_cursor = nil;
static BOOL expected_selection_move_cursor = NO;

static void guarded_cursor_set(NSCursor *cursor, SEL selector) {
  if (expected_selection_cursor != nil && cursor != expected_selection_cursor) {
    // AppKit has no public four-way move cursor. WebKit's CSS `move` cursor is
    // the system cursor the previous OSC used. Capture that native NSCursor
    // once while the pointer is over the selection body, then reuse it from
    // the native interaction view without routing any gesture through WebKit.
    if (expected_selection_move_cursor) {
      webkit_selection_move_cursor = cursor;
      expected_selection_cursor = cursor;
    } else {
      return;
    }
  }
  ((void (*)(id, SEL))original_cursor_set)(cursor, selector);
}

static void install_native_cursor_guard(void) {
  static dispatch_once_t once;
  dispatch_once(&once, ^{
    Method method = class_getInstanceMethod(NSCursor.class, @selector(set));
    original_cursor_set = method_setImplementation(
        method, (IMP)guarded_cursor_set);
  });
}

static NSRect editor_frame_with_transform(
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

static NSRect editor_frame(ScreenwidePreviewSurface *surface, NSRect base) {
  return editor_frame_with_transform(
      surface, base, surface.editorZoom,
      NSMakePoint(surface.editorPanX, surface.editorPanY));
}

/// Re-express a resized workspace against its new fit rectangle without
/// changing a single displayed pixel. Frame gestures use their immutable
/// starting transform to produce `displayed`; this function changes only the
/// fit-relative zoom/pan representation used by subsequent gestures and the
/// toolbar.
static NSRect rebase_workspace_fit(ScreenwidePreviewSurface *surface,
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

static void remember_workspace_transform(ScreenwidePreviewSurface *surface,
                                         double width, double height) {
  if (width <= 0.0 || height <= 0.0) return;
  ScreenwideWorkspaceTransform transform = {
    surface.editorZoom, surface.editorPanX, surface.editorPanY,
  };
  surface.workspaceTransforms[workspace_size_key(width, height)] =
      [NSValue valueWithBytes:&transform objCType:@encode(ScreenwideWorkspaceTransform)];
}

static void restore_workspace_transform(ScreenwidePreviewSurface *surface,
                                        double width, double height) {
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

typedef struct {
  float x;
  float y;
} ScreenwideSelectionPoint;

typedef struct {
  ScreenwideSelectionPoint position;
  ScreenwideSelectionPoint uv;
  uint32_t kind;
  uint32_t padding;
} ScreenwideSelectionVertex;

_Static_assert(sizeof(ScreenwideSelectionVertex) == 24,
               "Selection vertices must match the Metal struct stride");

static ScreenwideSelectionPoint selection_ndc(NSSize size, CGFloat x, CGFloat y) {
  return (ScreenwideSelectionPoint){
    (float)(2.0 * x / MAX(size.width, 1.0) - 1.0),
    (float)(1.0 - 2.0 * y / MAX(size.height, 1.0)),
  };
}

static void add_selection_quad(ScreenwideSelectionVertex *vertices, NSUInteger *count,
                               NSSize viewSize, NSRect rect, uint32_t kind) {
  ScreenwideSelectionPoint a = selection_ndc(viewSize, NSMinX(rect), NSMinY(rect));
  ScreenwideSelectionPoint b = selection_ndc(viewSize, NSMaxX(rect), NSMinY(rect));
  ScreenwideSelectionPoint c = selection_ndc(viewSize, NSMaxX(rect), NSMaxY(rect));
  ScreenwideSelectionPoint d = selection_ndc(viewSize, NSMinX(rect), NSMaxY(rect));
  ScreenwideSelectionVertex quad[6] = {
    {a, {0, 0}, kind, 0}, {b, {1, 0}, kind, 0}, {c, {1, 1}, kind, 0},
    {a, {0, 0}, kind, 0}, {c, {1, 1}, kind, 0}, {d, {0, 1}, kind, 0},
  };
  memcpy(vertices + *count, quad, sizeof(quad));
  *count += 6;
}

static void add_selection_pattern_quad(ScreenwideSelectionVertex *vertices,
                                       NSUInteger *count, NSSize viewSize,
                                       NSRect rect, uint32_t kind,
                                       BOOL horizontal, CGFloat scale) {
  ScreenwideSelectionPoint a = selection_ndc(viewSize, NSMinX(rect), NSMinY(rect));
  ScreenwideSelectionPoint b = selection_ndc(viewSize, NSMaxX(rect), NSMinY(rect));
  ScreenwideSelectionPoint c = selection_ndc(viewSize, NSMaxX(rect), NSMaxY(rect));
  ScreenwideSelectionPoint d = selection_ndc(viewSize, NSMinX(rect), NSMaxY(rect));
  float repeats = (float)((horizontal ? rect.size.width : rect.size.height) *
                          scale / 10.0);
  ScreenwideSelectionPoint uvA = {0, 0};
  ScreenwideSelectionPoint uvB = horizontal
      ? (ScreenwideSelectionPoint){repeats, 0}
      : (ScreenwideSelectionPoint){0, 0};
  ScreenwideSelectionPoint uvC = {repeats, repeats};
  ScreenwideSelectionPoint uvD = horizontal
      ? (ScreenwideSelectionPoint){0, 0}
      : (ScreenwideSelectionPoint){0, repeats};
  ScreenwideSelectionVertex quad[6] = {
    {a, uvA, kind, 0}, {b, uvB, kind, 0}, {c, uvC, kind, 0},
    {a, uvA, kind, 0}, {c, uvC, kind, 0}, {d, uvD, kind, 0},
  };
  memcpy(vertices + *count, quad, sizeof(quad));
  *count += 6;
}

static void add_selection_circle(ScreenwideSelectionVertex *vertices,
                                 NSUInteger *count, NSSize viewSize,
                                 NSPoint center, CGFloat radius,
                                 CGFloat margin, uint32_t kind) {
  CGFloat extent = radius + margin;
  NSRect rect = NSMakeRect(center.x - extent, center.y - extent,
                           extent * 2.0, extent * 2.0);
  ScreenwideSelectionPoint a = selection_ndc(viewSize, NSMinX(rect), NSMinY(rect));
  ScreenwideSelectionPoint b = selection_ndc(viewSize, NSMaxX(rect), NSMinY(rect));
  ScreenwideSelectionPoint c = selection_ndc(viewSize, NSMaxX(rect), NSMaxY(rect));
  ScreenwideSelectionPoint d = selection_ndc(viewSize, NSMinX(rect), NSMaxY(rect));
  float uvMargin = (float)(margin / (radius * 2.0));
  float uvMin = -uvMargin;
  float uvMax = 1.0f + uvMargin;
  ScreenwideSelectionVertex quad[6] = {
    {a, {uvMin, uvMin}, kind, 0}, {b, {uvMax, uvMin}, kind, 0},
    {c, {uvMax, uvMax}, kind, 0}, {a, {uvMin, uvMin}, kind, 0},
    {c, {uvMax, uvMax}, kind, 0}, {d, {uvMin, uvMax}, kind, 0},
  };
  memcpy(vertices + *count, quad, sizeof(quad));
  *count += 6;
}

static CGFloat selection_snap(CGFloat value, CGFloat scale) {
  return (floor(value * scale) + 0.5) / scale;
}

static void add_selection_osc(ScreenwideSelectionVertex *vertices,
                              NSUInteger *count, NSSize size, NSRect frame,
                              CGFloat scale, double radiusPercent,
                              BOOL radiusEnabled) {
  CGFloat minX = selection_snap(NSMinX(frame), scale);
  CGFloat maxX = selection_snap(NSMaxX(frame), scale);
  CGFloat minY = selection_snap(NSMinY(frame), scale);
  CGFloat maxY = selection_snap(NSMaxY(frame), scale);
  CGFloat midX = selection_snap((minX + maxX) / 2.0, scale);
  CGFloat midY = selection_snap((minY + maxY) / 2.0, scale);
  NSPoint points[8] = {
    NSMakePoint(minX, minY), NSMakePoint(midX, minY),
    NSMakePoint(maxX, minY), NSMakePoint(maxX, midY),
    NSMakePoint(maxX, maxY), NSMakePoint(midX, maxY),
    NSMakePoint(minX, maxY), NSMakePoint(minX, midY),
  };
  for (NSUInteger pass = 0; pass < 2; pass++) {
    BOOL halo = pass == 0;
    CGFloat lineHalf = (halo ? 1.5 : 0.5) / scale;
    uint32_t rectKind = halo ? 2 : 0;
    uint32_t circleKind = halo ? 3 : 1;
    add_selection_quad(vertices, count, size,
                       NSMakeRect(minX - lineHalf, minY - lineHalf,
                                  maxX - minX + lineHalf * 2.0,
                                  lineHalf * 2.0), rectKind);
    add_selection_quad(vertices, count, size,
                       NSMakeRect(minX - lineHalf, maxY - lineHalf,
                                  maxX - minX + lineHalf * 2.0,
                                  lineHalf * 2.0), rectKind);
    add_selection_quad(vertices, count, size,
                       NSMakeRect(minX - lineHalf, minY - lineHalf,
                                  lineHalf * 2.0,
                                  maxY - minY + lineHalf * 2.0), rectKind);
    add_selection_quad(vertices, count, size,
                       NSMakeRect(maxX - lineHalf, minY - lineHalf,
                                  lineHalf * 2.0,
                                  maxY - minY + lineHalf * 2.0), rectKind);
    CGFloat radius = 4.0 + (halo ? 1.0 / scale : 0.0);
    for (NSUInteger index = 0; index < 8; index++)
      add_selection_circle(vertices, count, size, points[index], radius,
                           1.0 / scale, circleKind);
    if (radiusEnabled) {
      CGFloat radiusOffset = MIN(maxX - minX, maxY - minY) *
                             radiusPercent / 100.0 * 0.55 + 10.0;
      add_selection_circle(vertices, count, size,
                           NSMakePoint(minX + radiusOffset,
                                       minY + radiusOffset),
                           radius, 1.0 / scale, circleKind);
    }
  }
}

static void add_crop_osc(ScreenwideSelectionVertex *vertices,
                         NSUInteger *count, NSSize size, NSRect crop,
                         NSRect image, CGFloat scale) {
  NSRect shade[4] = {
    NSMakeRect(NSMinX(image), NSMinY(image), image.size.width,
               MAX(NSMinY(crop) - NSMinY(image), 0.0)),
    NSMakeRect(NSMinX(image), NSMaxY(crop), image.size.width,
               MAX(NSMaxY(image) - NSMaxY(crop), 0.0)),
    NSMakeRect(NSMinX(image), NSMinY(crop),
               MAX(NSMinX(crop) - NSMinX(image), 0.0), crop.size.height),
    NSMakeRect(NSMaxX(crop), NSMinY(crop),
               MAX(NSMaxX(image) - NSMaxX(crop), 0.0), crop.size.height),
  };
  for (NSUInteger index = 0; index < 4; index++)
    if (!NSIsEmptyRect(shade[index]))
      add_selection_quad(vertices, count, size, shade[index], 6);

  CGFloat minX = selection_snap(NSMinX(crop), scale);
  CGFloat maxX = selection_snap(NSMaxX(crop), scale);
  CGFloat minY = selection_snap(NSMinY(crop), scale);
  CGFloat maxY = selection_snap(NSMaxY(crop), scale);
  CGFloat midX = selection_snap((minX + maxX) / 2.0, scale);
  CGFloat midY = selection_snap((minY + maxY) / 2.0, scale);
  NSPoint points[8] = {
    NSMakePoint(minX, minY), NSMakePoint(midX, minY),
    NSMakePoint(maxX, minY), NSMakePoint(maxX, midY),
    NSMakePoint(maxX, maxY), NSMakePoint(midX, maxY),
    NSMakePoint(minX, maxY), NSMakePoint(minX, midY),
  };
  for (NSUInteger pass = 0; pass < 2; pass++) {
    BOOL halo = pass == 0;
    CGFloat lineHalf = (halo ? 1.5 : 0.5) / scale;
    uint32_t horizontalKind = halo ? 8 : 7;
    uint32_t verticalKind = halo ? 10 : 9;
    add_selection_pattern_quad(
        vertices, count, size,
        NSMakeRect(minX - lineHalf, minY - lineHalf,
                   maxX - minX + lineHalf * 2.0, lineHalf * 2.0),
        horizontalKind, YES, scale);
    add_selection_pattern_quad(
        vertices, count, size,
        NSMakeRect(minX - lineHalf, maxY - lineHalf,
                   maxX - minX + lineHalf * 2.0, lineHalf * 2.0),
        horizontalKind, YES, scale);
    add_selection_pattern_quad(
        vertices, count, size,
        NSMakeRect(minX - lineHalf, minY - lineHalf, lineHalf * 2.0,
                   maxY - minY + lineHalf * 2.0),
        verticalKind, NO, scale);
    add_selection_pattern_quad(
        vertices, count, size,
        NSMakeRect(maxX - lineHalf, minY - lineHalf, lineHalf * 2.0,
                   maxY - minY + lineHalf * 2.0),
        verticalKind, NO, scale);
    CGFloat radius = 4.0 + (halo ? 1.0 / scale : 0.0);
    for (NSUInteger index = 0; index < 8; index++)
      add_selection_circle(vertices, count, size, points[index], radius,
                           1.0 / scale, halo ? 3 : 1);
  }
}

static NSRect selection_display_frame_for(
    ScreenwidePreviewSurface *surface,
    ScreenwidePreviewSelection selection);
static NSRect selection_image_frame_for(
    ScreenwidePreviewSurface *surface,
    ScreenwidePreviewSelection selection);
static BOOL selection_is_frame(ScreenwidePreviewSurface *surface);
static void redraw_workspace(ScreenwidePreviewSurface *surface);
static void update_crop_magnifier(ScreenwidePreviewSurface *surface,
                                  NSPoint point, uint32_t edges);
static void begin_workspace_frame_resize(ScreenwidePreviewSurface *surface);
static void update_workspace_frame_resize(
    ScreenwidePreviewSurface *surface, NSRect start, NSRect resized);
static BOOL update_workspace_auto_fit_move(
    ScreenwidePreviewSurface *surface, uint32_t selected_layer,
    double move_x, double move_y, NSRect start, NSRect resized);
static void end_workspace_frame_resize(
    ScreenwidePreviewSurface *surface, BOOL commit);

static void reflow_recording_workspace_panes(
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

static void rebase_recording_workspace_fit(
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

static const uint32_t ScreenwideAutoFitMoveEdge = 1u << 17;
static const uint32_t ScreenwideAutoFitCommitEdge = 1u << 18;

static void redraw_selection(ScreenwidePreviewSurface *surface) {
  surface.selectionDrawRevision += 1;
  uint64_t revision = surface.selectionDrawRevision;
  BOOL workspaceEncoding = surface.workspaceMode &&
      surface.workspaceEncodingCommand != nil &&
      surface.workspaceEncodingTexture != nil;
  if (surface.workspaceMode && !workspaceEncoding) {
    surface.selectionLayer.hidden = YES;
    redraw_workspace(surface);
    return;
  }
  BOOL selectedPaneActive = surface.workspaceMode
      ? [surface.workspaceActivePaneIndices
            containsObject:@(surface.selection.pane_index)]
      : surface.selection.pane_index < surface.views.count &&
            surface.views[surface.selection.pane_index].active;
  if (!surface.hasSelection || !surface.selectionVisible ||
      !surface.editorEnabled ||
      surface.selectionLayer == nil || surface.selectionPipeline == nil ||
      surface.selection.pane_index >= surface.editorBaseRects.count ||
      !selectedPaneActive) {
    surface.selectionDrawPending = NO;
    surface.selectionLayer.hidden = YES;
    return;
  }
  // Keep at most one OSC drawable in flight. `nextDrawable` otherwise waits
  // for display presentation when pointer events arrive faster than the
  // monitor refreshes, blocking AppKit for most of a frame on every move and
  // eventually hitting CAMetalLayer's one-second drawable timeout. A newer
  // gesture sample simply replaces the pending draw.
  if (!workspaceEncoding && surface.selectionDrawInFlight) {
    surface.selectionDrawPending = YES;
    return;
  }
  if (!workspaceEncoding) {
    surface.selectionDrawInFlight = YES;
    surface.selectionDrawPending = NO;
  }
  NSSize size = surface.interaction.bounds.size;
  NSRect pane = surface.editorBaseRects[surface.selection.pane_index].rectValue;
  NSRect base = NSMakeRect(pane.origin.x + pane.size.width * surface.selection.x,
                           pane.origin.y + pane.size.height * surface.selection.y,
                           pane.size.width * surface.selection.width,
                           pane.size.height * surface.selection.height);
  NSRect transformed = editor_frame(surface, base);
  NSRect frame = NSMakeRect(transformed.origin.x,
                            size.height - transformed.origin.y - transformed.size.height,
                            transformed.size.width, transformed.size.height);
  CGFloat scale = surface.host.window.backingScaleFactor ?: 1.0;
  ScreenwideSelectionVertex vertices[512];
  NSUInteger count = 0;
  // Match Keyframeless's contrast-safe OSC construction: hard-edged quads
  // snapped to drawable-pixel centres, with a 3px dark halo underneath a 1px
  // white core. Handles keep their 8pt fill and gain a 1-device-pixel ring.
  if (surface.selection.crop_mode != 0)
    add_crop_osc(vertices, &count, size, frame,
                 selection_image_frame_for(surface, surface.selection), scale);
  else
    add_selection_osc(vertices, &count, size, frame, scale,
                      surface.selection.radius_percent,
                      surface.selection.radius_disabled == 0);
  if (surface.hasSelectionSnapGuideX) {
    ScreenwidePreviewSelection guide = surface.selection;
    guide.x = surface.selectionSnapGuideX;
    guide.y = 0.0;
    guide.width = 0.0;
    guide.height = 0.0;
    CGFloat x = selection_snap(NSMinX(selection_display_frame_for(surface, guide)), scale);
    CGFloat half = 0.5 / scale;
    add_selection_quad(vertices, &count, size,
                       NSMakeRect(x - half, 0.0, half * 2.0, size.height),
                       surface.selectionSnapGuideXIsObject ? 5 : 4);
  }
  if (surface.hasSelectionSnapGuideY) {
    ScreenwidePreviewSelection guide = surface.selection;
    guide.x = 0.0;
    guide.y = surface.selectionSnapGuideY;
    guide.width = 0.0;
    guide.height = 0.0;
    CGFloat y = selection_snap(NSMinY(selection_display_frame_for(surface, guide)), scale);
    CGFloat half = 0.5 / scale;
    add_selection_quad(vertices, &count, size,
                       NSMakeRect(0.0, y - half, size.width, half * 2.0),
                       surface.selectionSnapGuideYIsObject ? 5 : 4);
  }
  if (workspaceEncoding) {
    id<MTLBuffer> buffer = [surface.device newBufferWithBytes:vertices
        length:count * sizeof(*vertices)
        options:MTLResourceStorageModeShared];
    MTLRenderPassDescriptor *pass = [MTLRenderPassDescriptor renderPassDescriptor];
    pass.colorAttachments[0].texture = surface.workspaceEncodingTexture;
    pass.colorAttachments[0].loadAction = MTLLoadActionLoad;
    pass.colorAttachments[0].storeAction = MTLStoreActionStore;
    id<MTLRenderCommandEncoder> encoder =
        [surface.workspaceEncodingCommand renderCommandEncoderWithDescriptor:pass];
    [encoder setRenderPipelineState:surface.selectionPipeline];
    [encoder setVertexBuffer:buffer offset:0 atIndex:0];
    NSString *appearance = [surface.interaction.effectiveAppearance
        bestMatchFromAppearancesWithNames:@[NSAppearanceNameAqua,
                                            NSAppearanceNameDarkAqua]];
    uint32_t lightMode =
        [appearance isEqualToString:NSAppearanceNameAqua] ? 1 : 0;
    [encoder setFragmentBytes:&lightMode length:sizeof(lightMode) atIndex:0];
    ScreenwideWorkspaceMagnifier magnifier = surface.workspaceMagnifier;
    float magnifierBox[4] = {
      magnifier.active != 0 ? magnifier.box_x : 0,
      magnifier.active != 0 ? magnifier.box_y : 0,
      magnifier.active != 0 ? magnifier.box_width : 0,
      magnifier.active != 0 ? magnifier.box_height : 0,
    };
    [encoder setFragmentBytes:magnifierBox length:sizeof(magnifierBox) atIndex:1];
    [encoder drawPrimitives:MTLPrimitiveTypeTriangle
                vertexStart:0 vertexCount:count];
    [encoder endEncoding];
    return;
  }
  surface.selectionLayer.frame = surface.interaction.bounds;
  surface.selectionLayer.contentsScale = scale;
  surface.selectionLayer.drawableSize = CGSizeMake(MAX(size.width * scale, 2.0),
                                                    MAX(size.height * scale, 2.0));
  id<CAMetalDrawable> drawable = [surface.selectionLayer nextDrawable];
  if (drawable == nil) {
    surface.selectionDrawInFlight = NO;
    return;
  }
  id<MTLBuffer> buffer = [surface.device newBufferWithBytes:vertices
                                                      length:count * sizeof(*vertices)
                                                     options:MTLResourceStorageModeShared];
  MTLRenderPassDescriptor *pass = [MTLRenderPassDescriptor renderPassDescriptor];
  pass.colorAttachments[0].texture = drawable.texture;
  pass.colorAttachments[0].loadAction = MTLLoadActionClear;
  pass.colorAttachments[0].storeAction = MTLStoreActionStore;
  pass.colorAttachments[0].clearColor = MTLClearColorMake(0, 0, 0, 0);
  id<MTLCommandBuffer> command = [surface.queue commandBuffer];
  id<MTLRenderCommandEncoder> encoder = [command renderCommandEncoderWithDescriptor:pass];
  [encoder setRenderPipelineState:surface.selectionPipeline];
  [encoder setVertexBuffer:buffer offset:0 atIndex:0];
  NSString *appearance = [surface.interaction.effectiveAppearance
      bestMatchFromAppearancesWithNames:@[NSAppearanceNameAqua,
                                          NSAppearanceNameDarkAqua]];
  uint32_t lightMode = [appearance isEqualToString:NSAppearanceNameAqua] ? 1 : 0;
  [encoder setFragmentBytes:&lightMode length:sizeof(lightMode) atIndex:0];
  float magnifierBox[4] = {0};
  [encoder setFragmentBytes:magnifierBox length:sizeof(magnifierBox) atIndex:1];
  [encoder drawPrimitives:MTLPrimitiveTypeTriangle vertexStart:0 vertexCount:count];
  [encoder endEncoding];
  [command presentDrawable:drawable];
  [command addCompletedHandler:^(__unused id<MTLCommandBuffer> completed) {
    dispatch_async(dispatch_get_main_queue(), ^{
      surface.selectionDrawInFlight = NO;
      BOOL redrawPending = surface.selectionDrawPending;
      surface.selectionDrawPending = NO;
      if (surface.hasSelection && surface.selectionVisible &&
          surface.editorEnabled)
        surface.selectionLayer.hidden = NO;
      if (redrawPending) {
        redraw_selection(surface);
      } else if (surface.selectionDrawRevision == revision &&
                 surface.hasSelection && surface.selectionVisible &&
                 surface.editorEnabled) {
        surface.selectionLayer.hidden = NO;
      }
    });
  }];
  [command commit];
}

static void invalidate_selection_cursor_rects(ScreenwidePreviewSurface *surface);

static void apply_editor_transform(ScreenwidePreviewSurface *surface) {
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

static NSRect selection_display_frame_for(ScreenwidePreviewSurface *surface,
                                          ScreenwidePreviewSelection selection) {
  if (selection.pane_index >= surface.editorBaseRects.count)
    return NSZeroRect;
  NSRect pane = surface.editorBaseRects[selection.pane_index].rectValue;
  NSRect base = NSMakeRect(pane.origin.x + pane.size.width * selection.x,
                           pane.origin.y + pane.size.height * selection.y,
                           pane.size.width * selection.width,
                           pane.size.height * selection.height);
  NSRect transformed = editor_frame(surface, base);
  return NSMakeRect(transformed.origin.x,
                    surface.interaction.bounds.size.height - transformed.origin.y - transformed.size.height,
                    transformed.size.width, transformed.size.height);
}

static NSRect selection_display_frame(ScreenwidePreviewSurface *surface) {
  if (!surface.hasSelection) return NSZeroRect;
  return selection_display_frame_for(surface, surface.selection);
}

static NSRect selection_image_frame_for(
    ScreenwidePreviewSurface *surface,
    ScreenwidePreviewSelection selection) {
  ScreenwidePreviewSelection image = selection;
  image.x = selection.image_x;
  image.y = selection.image_y;
  image.width = selection.image_width;
  image.height = selection.image_height;
  return selection_display_frame_for(surface, image);
}

static NSRect auto_fit_selection_bounds(
    ScreenwidePreviewSurface *surface,
    NSArray<NSValue *> *targets,
    ScreenwidePreviewSelection moved) {
  double left = 0.0, top = 0.0, right = 1.0, bottom = 1.0;
  for (NSValue *value in targets) {
    ScreenwidePreviewSelection target;
    [value getValue:&target size:sizeof(target)];
    if (target.layer_id == moved.layer_id) target = moved;
    left = MIN(left, target.x);
    top = MIN(top, target.y);
    right = MAX(right, target.x + target.width);
    bottom = MAX(bottom, target.y + target.height);
  }
  left = MIN(left, moved.x);
  top = MIN(top, moved.y);
  right = MAX(right, moved.x + moved.width);
  bottom = MAX(bottom, moved.y + moved.height);
  double naturalWidth = MAX(surface.workspaceResizeNaturalWidth, 1.0);
  double naturalHeight = MAX(surface.workspaceResizeNaturalHeight, 1.0);
  left = floor(left * naturalWidth) / naturalWidth;
  top = floor(top * naturalHeight) / naturalHeight;
  right = ceil(right * naturalWidth) / naturalWidth;
  bottom = ceil(bottom * naturalHeight) / naturalHeight;
  return NSMakeRect(left, top, MAX(right - left, 0.000001),
                    MAX(bottom - top, 0.000001));
}

static BOOL selection_is_frame(ScreenwidePreviewSurface *surface) {
  return surface.hasSelection &&
         surface.selection.layer_id == ScreenwideFrameLayerId;
}

static BOOL selection_target_at_point(ScreenwidePreviewSurface *surface,
                                      NSPoint point,
                                      ScreenwidePreviewSelection *result) {
  for (NSValue *value in surface.selectionTargets.reverseObjectEnumerator) {
    ScreenwidePreviewSelection target;
    [value getValue:&target size:sizeof(target)];
    BOOL paneActive = surface.workspaceMode
        ? [surface.workspaceActivePaneIndices containsObject:@(target.pane_index)]
        : target.pane_index < surface.views.count &&
              surface.views[target.pane_index].active;
    if (paneActive &&
        NSPointInRect(point, selection_display_frame_for(surface, target))) {
      *result = target;
      return YES;
    }
  }
  return NO;
}

static uint64_t selection_target_id(ScreenwidePreviewSelection target) {
  return ((uint64_t)target.pane_index << 32) | (uint64_t)target.layer_id;
}

static BOOL shared_selection_hit(ScreenwidePreviewSurface *surface,
                                 NSPoint point,
                                 ScreenwidePreviewSelection *selection,
                                 uint8_t *handle) {
  NSUInteger capacity = surface.selectionTargets.count + 1;
  if (capacity == 0) return NO;
  ScreenwideDisplayTarget *targets =
      calloc(capacity, sizeof(ScreenwideDisplayTarget));
  if (targets == NULL) return NO;
  NSUInteger count = 0;
  BOOL includedSelection = NO;
  for (NSValue *value in surface.selectionTargets) {
    ScreenwidePreviewSelection target;
    [value getValue:&target size:sizeof(target)];
    BOOL selected = surface.hasSelection &&
                    target.pane_index == surface.selection.pane_index &&
                    target.layer_id == surface.selection.layer_id;
    if (selected) {
      target = surface.selection;
      includedSelection = YES;
    }
    BOOL visible = surface.workspaceMode
        ? [surface.workspaceActivePaneIndices containsObject:@(target.pane_index)]
        : target.pane_index < surface.views.count &&
              surface.views[target.pane_index].active;
    NSRect rect = selection_display_frame_for(surface, target);
    int32_t zOrder = (int32_t)count;
    targets[count] = (ScreenwideDisplayTarget){
        .id = selection_target_id(target),
        .rect = {rect.origin.x, rect.origin.y, rect.size.width,
                 rect.size.height},
        .radius_enabled = target.crop_mode == 0 && target.radius_disabled == 0 ? 1 : 0,
        .radius_percent = target.radius_percent,
        .z_order = zOrder,
        .selected = selected ? 1 : 0,
        .visible = visible ? 1 : 0,
    };
    count++;
  }
  if (surface.hasSelection && !includedSelection) {
    ScreenwidePreviewSelection target = surface.selection;
    NSRect rect = selection_display_frame_for(surface, target);
    targets[count++] = (ScreenwideDisplayTarget){
        .id = selection_target_id(target),
        .rect = {rect.origin.x, rect.origin.y, rect.size.width,
                 rect.size.height},
        .radius_enabled = target.crop_mode == 0 && target.radius_disabled == 0 ? 1 : 0,
        .radius_percent = target.radius_percent,
        .z_order = INT32_MAX,
        .selected = 1,
        .visible = 1,
    };
  }
  ScreenwideDisplayHit hit = screenwide_workspace_hit_test(
      targets, count, point.x, point.y, 8.0);
  free(targets);
  if (!hit.found) return NO;
  uint32_t paneIndex = (uint32_t)(hit.target_id >> 32);
  uint32_t layerId = (uint32_t)hit.target_id;
  if (surface.hasSelection &&
      surface.selection.pane_index == paneIndex &&
      surface.selection.layer_id == layerId) {
    *selection = surface.selection;
  } else {
    BOOL found = NO;
    for (NSValue *value in surface.selectionTargets) {
      ScreenwidePreviewSelection target;
      [value getValue:&target size:sizeof(target)];
      if (target.pane_index == paneIndex && target.layer_id == layerId) {
        *selection = target;
        found = YES;
        break;
      }
    }
    if (!found) return NO;
  }
  *handle = hit.handle;
  return YES;
}

static uint32_t shared_handle_edges(uint8_t handle) {
  switch (handle) {
    case 1: return 4;
    case 2: return 8;
    case 3: return 2;
    case 4: return 1;
    case 5: return 2 | 4;
    case 6: return 1 | 4;
    case 7: return 2 | 8;
    case 8: return 1 | 8;
    default: return 0;
  }
}

static void emit_selection_gesture(ScreenwidePreviewSurface *surface,
                                   uint32_t phase, uint32_t operation,
                                   uint32_t edges, double scale,
                                   double deltaX, double deltaY) {
  if (surface.selectionGestureCallback)
    surface.selectionGestureCallback(
                                     phase,
                                     operation == 3 || operation == 4
                                         ? surface.selection.pane_index
                                         : surface.selection.layer_id,
                                     operation, edges, scale, deltaX, deltaY,
                                     surface.selectionGestureContext);
}

// Selection edges use the same names as the DOM implementation: left=1,
// right=2, top=4, bottom=8. Hit regions are 16 points square around each
// visible four-point handle and are checked before the selection body.
static uint32_t selection_handle_edges(ScreenwidePreviewSurface *surface,
                                       NSPoint point) {
  NSRect frame = selection_display_frame(surface);
  if (NSIsEmptyRect(frame)) return 0;
  NSPoint handles[8] = {
    NSMakePoint(NSMinX(frame), NSMinY(frame)),
    NSMakePoint(NSMidX(frame), NSMinY(frame)),
    NSMakePoint(NSMaxX(frame), NSMinY(frame)),
    NSMakePoint(NSMaxX(frame), NSMidY(frame)),
    NSMakePoint(NSMaxX(frame), NSMaxY(frame)),
    NSMakePoint(NSMidX(frame), NSMaxY(frame)),
    NSMakePoint(NSMinX(frame), NSMaxY(frame)),
    NSMakePoint(NSMinX(frame), NSMidY(frame)),
  };
  static const uint32_t edges[8] = { 1 | 4, 4, 2 | 4, 2,
                                     2 | 8, 8, 1 | 8, 1 };
  for (NSUInteger index = 0; index < 8; index++) {
    if (fabs(point.x - handles[index].x) <= 8.0 &&
        fabs(point.y - handles[index].y) <= 8.0)
      return edges[index];
  }
  return 0;
}

static NSPoint selection_radius_point(ScreenwidePreviewSurface *surface) {
  NSRect frame = selection_display_frame(surface);
  double offset = MIN(frame.size.width, frame.size.height) *
                  surface.selection.radius_percent / 100.0 * 0.55 + 10.0;
  return NSMakePoint(NSMinX(frame) + offset, NSMinY(frame) + offset);
}

static BOOL selection_radius_hit(ScreenwidePreviewSurface *surface,
                                 NSPoint point) {
  if (!surface.hasSelection || surface.selection.crop_mode != 0) return NO;
  NSPoint radius = selection_radius_point(surface);
  return fabs(point.x - radius.x) <= 8.0 && fabs(point.y - radius.y) <= 8.0;
}

typedef struct {
  BOOL found;
  BOOL object;
  double adjustment;
  double distance;
  double guide;
} ScreenwideSelectionSnap;

static void consider_selection_snap(ScreenwideSelectionSnap *best,
                                    double adjustment, double guide,
                                    BOOL object, double threshold) {
  double distance = fabs(adjustment);
  if (distance > threshold ||
      (best->found && (distance > best->distance ||
                       (distance == best->distance && !object))))
    return;
  best->found = YES;
  best->object = object;
  best->adjustment = adjustment;
  best->distance = distance;
  best->guide = guide;
}

static BOOL selection_target_shares_frame(
    ScreenwidePreviewSurface *surface,
    ScreenwidePreviewSelection target) {
  ScreenwidePreviewSelection start = surface.interaction.selectionDragStart;
  if (start.pane_index >= surface.editorBaseRects.count ||
      target.pane_index >= surface.editorBaseRects.count)
    return NO;
  return NSEqualRects(surface.editorBaseRects[start.pane_index].rectValue,
                      surface.editorBaseRects[target.pane_index].rectValue);
}

static ScreenwideSelectionSnap selection_snap_axis(
    ScreenwidePreviewSurface *surface, BOOL horizontal,
    double position, double extent, NSRect pane) {
  ScreenwideSelectionSnap best = {0};
  double paneExtent = horizontal ? pane.size.width : pane.size.height;
  double threshold = 8.0 / MAX(paneExtent * surface.editorZoom, 1.0);
  double inset = MIN(pane.size.width, pane.size.height) * 0.02 /
                 MAX(paneExtent, 1.0);
  double maximum = 1.0 - extent;
  double placements[3] = {
    maximum >= 0.0 ? MIN(inset, maximum) : 0.0,
    maximum / 2.0,
    maximum >= 0.0 ? MAX(0.0, maximum - inset) : maximum,
  };
  for (NSUInteger index = 0; index < 3; index++) {
    double guide = index == 0 ? placements[index]
                              : index == 1 ? 0.5
                                           : placements[index] + extent;
    consider_selection_snap(&best, placements[index] - position, guide,
                            NO, threshold);
  }
  double moving[3] = {position, position + extent / 2.0,
                      position + extent};
  for (NSValue *value in surface.selectionTargets) {
    ScreenwidePreviewSelection target;
    [value getValue:&target size:sizeof(target)];
    if (target.layer_id == surface.interaction.selectionDragStart.layer_id ||
        !selection_target_shares_frame(surface, target))
      continue;
    double targetOrigin = horizontal ? target.x : target.y;
    double targetExtent = horizontal ? target.width : target.height;
    double targets[3] = {targetOrigin, targetOrigin + targetExtent / 2.0,
                         targetOrigin + targetExtent};
    for (NSUInteger movingIndex = 0; movingIndex < 3; movingIndex++)
      for (NSUInteger targetIndex = 0; targetIndex < 3; targetIndex++)
        consider_selection_snap(&best,
                                targets[targetIndex] - moving[movingIndex],
                                targets[targetIndex], YES, threshold);
  }
  return best;
}

static void consider_selection_resize_snap(
    ScreenwideSelectionSnap *best, double candidateScale, double guide,
    BOOL object, double handleDistance, double paneExtent,
    double threshold) {
  if (handleDistance > threshold) return;
  double distance = handleDistance * paneExtent;
  if (best->found &&
      (distance > best->distance ||
       (distance == best->distance && !object)))
    return;
  best->found = YES;
  best->object = object;
  best->adjustment = candidateScale;
  best->distance = distance;
  best->guide = guide;
}

static ScreenwideSelectionSnap selection_resize_snap_axis(
    ScreenwidePreviewSurface *surface, BOOL horizontal,
    double anchor, double vector, double rawScale, NSRect pane,
    double minimumScale, double maximumScale) {
  ScreenwideSelectionSnap best = {0};
  if (fabs(vector) < 0.0000001) return best;
  double paneExtent = horizontal ? pane.size.width : pane.size.height;
  double threshold = 8.0 / MAX(paneExtent * surface.editorZoom, 1.0);
  double inset = MIN(pane.size.width, pane.size.height) * 0.02 /
                 MAX(paneExtent, 1.0);
  double handle = anchor + vector * rawScale;
  double canvasTargets[3] = {inset, 0.5, 1.0 - inset};
  for (NSUInteger index = 0; index < 3; index++) {
    double candidateScale = (canvasTargets[index] - anchor) / vector;
    if (candidateScale < minimumScale || candidateScale > maximumScale)
      continue;
    consider_selection_resize_snap(
        &best, candidateScale, canvasTargets[index], NO,
        fabs(canvasTargets[index] - handle), paneExtent, threshold);
  }
  for (NSValue *value in surface.selectionTargets) {
    ScreenwidePreviewSelection target;
    [value getValue:&target size:sizeof(target)];
    if (target.layer_id == surface.interaction.selectionDragStart.layer_id ||
        !selection_target_shares_frame(surface, target))
      continue;
    double origin = horizontal ? target.x : target.y;
    double extent = horizontal ? target.width : target.height;
    double targets[3] = {origin, origin + extent / 2.0, origin + extent};
    for (NSUInteger index = 0; index < 3; index++) {
      double candidateScale = (targets[index] - anchor) / vector;
      if (candidateScale < minimumScale || candidateScale > maximumScale)
        continue;
      double handleDistance = fabs(targets[index] - handle);
      if (handleDistance > threshold) continue;
      consider_selection_resize_snap(
          &best, candidateScale, targets[index], YES, handleDistance,
          paneExtent, threshold);
    }
  }
  return best;
}

static void clear_selection_snap_guides(ScreenwidePreviewSurface *surface);

static double snap_selection_resize(ScreenwidePreviewSurface *surface,
                                    double scale, double anchorX,
                                    double anchorY, double vectorX,
                                    double vectorY, uint32_t edges,
                                    NSRect pane, double minimumScale,
                                    double maximumScale) {
  ScreenwideSelectionSnap horizontal = {0};
  ScreenwideSelectionSnap vertical = {0};
  if ((edges & (1 | 2)) != 0)
    horizontal = selection_resize_snap_axis(
        surface, YES, anchorX, vectorX, scale, pane,
        minimumScale, maximumScale);
  if ((edges & (4 | 8)) != 0)
    vertical = selection_resize_snap_axis(
        surface, NO, anchorY, vectorY, scale, pane,
        minimumScale, maximumScale);
  ScreenwideSelectionSnap chosen = horizontal;
  if (!chosen.found ||
      (vertical.found && vertical.distance < chosen.distance))
    chosen = vertical;
  if (!chosen.found) {
    clear_selection_snap_guides(surface);
    return scale;
  }
  double snappedScale = chosen.adjustment;
  double xDifference = fabs(horizontal.adjustment - snappedScale) *
                       fabs(vectorX) * pane.size.width * surface.editorZoom;
  double yDifference = fabs(vertical.adjustment - snappedScale) *
                       fabs(vectorY) * pane.size.height * surface.editorZoom;
  surface.hasSelectionSnapGuideX = horizontal.found && xDifference <= 0.5;
  surface.hasSelectionSnapGuideY = vertical.found && yDifference <= 0.5;
  surface.selectionSnapGuideX = horizontal.guide;
  surface.selectionSnapGuideY = vertical.guide;
  surface.selectionSnapGuideXIsObject = horizontal.object;
  surface.selectionSnapGuideYIsObject = vertical.object;
  return snappedScale;
}

static void clear_selection_snap_guides(ScreenwidePreviewSurface *surface) {
  surface.hasSelectionSnapGuideX = NO;
  surface.hasSelectionSnapGuideY = NO;
}

static void snap_selection_move(ScreenwidePreviewSurface *surface,
                                double *x, double *y) {
  ScreenwidePreviewSelection start = surface.interaction.selectionDragStart;
  if (start.pane_index >= surface.editorBaseRects.count)
    return;
  NSRect pane = surface.editorBaseRects[start.pane_index].rectValue;
  ScreenwideSelectionSnap horizontal = selection_snap_axis(
      surface, YES, *x, start.width, pane);
  ScreenwideSelectionSnap vertical = selection_snap_axis(
      surface, NO, *y, start.height, pane);
  if (horizontal.found) *x += horizontal.adjustment;
  if (vertical.found) *y += vertical.adjustment;
  surface.hasSelectionSnapGuideX = horizontal.found;
  surface.hasSelectionSnapGuideY = vertical.found;
  surface.selectionSnapGuideX = horizontal.guide;
  surface.selectionSnapGuideY = vertical.guide;
  surface.selectionSnapGuideXIsObject = horizontal.object;
  surface.selectionSnapGuideYIsObject = vertical.object;
}

static NSCursor *selection_resize_cursor(uint32_t edges) {
  if (@available(macOS 15.0, *)) {
    NSCursorFrameResizePosition position = 0;
    if (edges & 1) position |= NSCursorFrameResizePositionLeft;
    if (edges & 2) position |= NSCursorFrameResizePositionRight;
    if (edges & 4) position |= NSCursorFrameResizePositionTop;
    if (edges & 8) position |= NSCursorFrameResizePositionBottom;
    if (position != 0)
      return [NSCursor frameResizeCursorFromPosition:position
                                        inDirections:NSCursorFrameResizeDirectionsAll];
  }
  static NSCursor *nwse = nil;
  static NSCursor *nesw = nil;
  static dispatch_once_t once;
  dispatch_once(&once, ^{
    NSImage *descending = [[NSImage alloc] initWithSize:NSMakeSize(16, 16)];
    [descending lockFocus];
    [[NSColor whiteColor] setStroke];
    NSBezierPath *outline = [NSBezierPath bezierPath];
    [outline setLineWidth:3.0];
    [outline moveToPoint:NSMakePoint(2, 14)]; [outline lineToPoint:NSMakePoint(14, 2)];
    [outline moveToPoint:NSMakePoint(2, 14)]; [outline lineToPoint:NSMakePoint(2, 9)];
    [outline moveToPoint:NSMakePoint(2, 14)]; [outline lineToPoint:NSMakePoint(7, 14)];
    [outline moveToPoint:NSMakePoint(14, 2)]; [outline lineToPoint:NSMakePoint(9, 2)];
    [outline moveToPoint:NSMakePoint(14, 2)]; [outline lineToPoint:NSMakePoint(14, 7)];
    [outline stroke];
    [[NSColor blackColor] setStroke];
    NSBezierPath *line = [NSBezierPath bezierPath];
    [line setLineWidth:1.0];
    [line moveToPoint:NSMakePoint(2, 14)]; [line lineToPoint:NSMakePoint(14, 2)];
    [line moveToPoint:NSMakePoint(2, 14)]; [line lineToPoint:NSMakePoint(2, 9)];
    [line moveToPoint:NSMakePoint(2, 14)]; [line lineToPoint:NSMakePoint(7, 14)];
    [line moveToPoint:NSMakePoint(14, 2)]; [line lineToPoint:NSMakePoint(9, 2)];
    [line moveToPoint:NSMakePoint(14, 2)]; [line lineToPoint:NSMakePoint(14, 7)];
    [line stroke];
    [descending unlockFocus];
    nwse = [[NSCursor alloc] initWithImage:descending hotSpot:NSMakePoint(8, 8)];
    NSImage *ascending = [[NSImage alloc] initWithSize:NSMakeSize(16, 16)];
    [ascending lockFocus];
    [[NSColor whiteColor] setStroke];
    NSBezierPath *outline2 = [NSBezierPath bezierPath];
    [outline2 setLineWidth:3.0];
    [outline2 moveToPoint:NSMakePoint(2, 2)]; [outline2 lineToPoint:NSMakePoint(14, 14)];
    [outline2 moveToPoint:NSMakePoint(2, 2)]; [outline2 lineToPoint:NSMakePoint(2, 7)];
    [outline2 moveToPoint:NSMakePoint(2, 2)]; [outline2 lineToPoint:NSMakePoint(7, 2)];
    [outline2 moveToPoint:NSMakePoint(14, 14)]; [outline2 lineToPoint:NSMakePoint(9, 14)];
    [outline2 moveToPoint:NSMakePoint(14, 14)]; [outline2 lineToPoint:NSMakePoint(14, 9)];
    [outline2 stroke];
    [[NSColor blackColor] setStroke];
    NSBezierPath *line2 = [NSBezierPath bezierPath];
    [line2 setLineWidth:1.0];
    [line2 moveToPoint:NSMakePoint(2, 2)]; [line2 lineToPoint:NSMakePoint(14, 14)];
    [line2 moveToPoint:NSMakePoint(2, 2)]; [line2 lineToPoint:NSMakePoint(2, 7)];
    [line2 moveToPoint:NSMakePoint(2, 2)]; [line2 lineToPoint:NSMakePoint(7, 2)];
    [line2 moveToPoint:NSMakePoint(14, 14)]; [line2 lineToPoint:NSMakePoint(9, 14)];
    [line2 moveToPoint:NSMakePoint(14, 14)]; [line2 lineToPoint:NSMakePoint(14, 9)];
    [line2 stroke];
    [ascending unlockFocus];
    nesw = [[NSCursor alloc] initWithImage:ascending hotSpot:NSMakePoint(8, 8)];
  });
  if (edges == (1 | 4) || edges == (2 | 8)) return nwse;
  if (edges == (2 | 4) || edges == (1 | 8)) return nesw;
  if (edges == 1 || edges == 2) return [NSCursor resizeLeftRightCursor];
  if (edges == 4 || edges == 8) return [NSCursor resizeUpDownCursor];
  return nil;
}

static NSCursor *selection_move_cursor(void) {
  if (webkit_selection_move_cursor != nil) return webkit_selection_move_cursor;
  static NSCursor *move = nil;
  static dispatch_once_t once;
  dispatch_once(&once, ^{
    NSImage *systemImage = [NSImage imageNamed:@"NSMoveCursor"];
    if (systemImage != nil) {
      move = [[NSCursor alloc] initWithImage:systemImage
                                    hotSpot:NSMakePoint(systemImage.size.width / 2.0,
                                                        systemImage.size.height / 2.0)];
    }
    if (move == nil) move = [NSCursor openHandCursor];
  });
  return move;
}

static NSCursor *selection_cursor(ScreenwidePreviewSurface *surface,
                                   NSPoint point) {
  ScreenwidePreviewSelection target;
  uint8_t handle = 0;
  if (!shared_selection_hit(surface, point, &target, &handle))
    return [NSCursor openHandCursor];
  BOOL inactiveFrame = target.layer_id == ScreenwideFrameLayerId &&
      (!surface.hasSelection ||
       surface.selection.pane_index != target.pane_index ||
       surface.selection.layer_id != target.layer_id);
  if (inactiveFrame ||
      (handle == 0 && target.layer_id == ScreenwideFrameLayerId))
    return [NSCursor arrowCursor];
  if (handle == 9) return selection_resize_cursor(1 | 4);
  NSCursor *resize = selection_resize_cursor(shared_handle_edges(handle));
  return resize != nil ? resize : selection_move_cursor();
}

static void set_selection_cursor(NSCursor *cursor) {
  expected_selection_move_cursor = NO;
  expected_selection_cursor = cursor;
  [cursor set];
}

static void set_selection_move_cursor(void) {
  expected_selection_move_cursor = YES;
  expected_selection_cursor = selection_move_cursor();
  [expected_selection_cursor set];
}

static void set_selection_cursor_at_point(ScreenwidePreviewSurface *surface,
                                          NSPoint point) {
  NSCursor *cursor = selection_cursor(surface, point);
  if (cursor == selection_move_cursor()) set_selection_move_cursor();
  else set_selection_cursor(cursor);
}

static void invalidate_selection_cursor_rects(ScreenwidePreviewSurface *surface) {
  if (surface.interaction.window != nil)
    [surface.interaction.window invalidateCursorRectsForView:surface.interaction];
}

static void set_editor_zoom(ScreenwidePreviewSurface *surface,
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
- (BOOL)isFlipped { return YES; }
- (BOOL)acceptsFirstResponder { return NO; }
- (BOOL)acceptsFirstMouse:(NSEvent *)event { (void)event; return YES; }
- (BOOL)mouseDownCanMoveWindow { return NO; }
- (void)viewDidChangeEffectiveAppearance {
  [super viewDidChangeEffectiveAppearance];
  if (self.surface != nil) redraw_selection(self.surface);
}
- (void)claimCursorControl {
  if (self.cursorRectsDisabled || self.window == nil) return;
  [self.window disableCursorRects];
  self.cursorRectsDisabled = YES;
}
- (void)beginWorkspaceMove {
  self.selectionMoveDeltaX = 0.0;
  self.selectionMoveDeltaY = 0.0;
  self.selectionMoveAutoFitActive = NO;
  self.selectionMoveAutoFitBounds = NSZeroRect;
  self.selectionMoveTargetsStart = [self.surface.selectionTargets copy];
  self.selectionMoveZoomStart = self.surface.editorZoom;
  self.selectionMovePanStart =
      NSMakePoint(self.surface.editorPanX, self.surface.editorPanY);
  self.selectionFramePaneStarts = [self.surface.editorBaseRects copy];
  if (!self.surface.workspaceMode || self.surface.editorBaseRects.count == 0) {
    self.selectionMoveFrameStart = NSZeroRect;
    return;
  }
  NSUInteger paneIndex = self.surface.selection.pane_index;
  self.selectionMoveFrameStart = paneIndex < self.surface.editorBaseRects.count
      ? self.surface.editorBaseRects[paneIndex].rectValue
      : NSZeroRect;
  begin_workspace_frame_resize(self.surface);
}
- (void)releaseCursorControl {
  if (!self.cursorRectsDisabled || self.window == nil) return;
  [self.window enableCursorRects];
  self.cursorRectsDisabled = NO;
  [self.window resetCursorRects];
  expected_selection_cursor = nil;
  expected_selection_move_cursor = NO;
}
- (void)updateTrackingAreas {
  [super updateTrackingAreas];
  if (self.selectionTrackingArea != nil)
    [self removeTrackingArea:self.selectionTrackingArea];
  self.selectionTrackingArea = [[NSTrackingArea alloc]
      initWithRect:self.bounds
           options:NSTrackingMouseMoved | NSTrackingActiveAlways |
                   NSTrackingMouseEnteredAndExited |
                   NSTrackingInVisibleRect |
                   NSTrackingCursorUpdate
             owner:self userInfo:nil];
  [self addTrackingArea:self.selectionTrackingArea];
}
- (void)mouseMoved:(NSEvent *)event {
  [self claimCursorControl];
  NSPoint point = [self convertPoint:event.locationInWindow fromView:nil];
  set_selection_cursor_at_point(self.surface, point);
}
- (void)mouseEntered:(NSEvent *)event { [self mouseMoved:event]; }
- (void)mouseExited:(NSEvent *)event {
  [self releaseCursorControl];
  [[NSCursor arrowCursor] set];
  (void)event;
}
- (void)resetCursorRects {
  // Cursor rects overlap at every handle and AppKit repeatedly restores the
  // workspace cursor after `mouseMoved:` selects a resize cursor. The tracking
  // area is the single cursor authority for this native workarea.
  [super resetCursorRects];
}
- (void)cursorUpdate:(NSEvent *)event {
  if (self.selectionDragActive &&
      (self.selectionDragOperation == 2 || self.selectionDragOperation == 4))
    set_selection_cursor(selection_resize_cursor(1 | 4));
  else if (self.selectionDragActive &&
           (self.selectionDragOperation == 1 || self.selectionDragOperation == 3 ||
            self.selectionDragOperation == 6))
    set_selection_cursor(selection_resize_cursor(self.selectionDragEdges));
  else if (self.selectionDragActive)
    set_selection_move_cursor();
  else if (self.panning)
    set_selection_cursor([NSCursor closedHandCursor]);
  else {
    NSPoint point = [self convertPoint:event.locationInWindow fromView:nil];
    set_selection_cursor_at_point(self.surface, point);
  }
  (void)event;
}
- (void)mouseDown:(NSEvent *)event {
  // Keep keyboard shortcuts in React even though pointer gestures are native.
  // The overlay receives the click, so AppKit cannot focus WKWebView for us.
  if (self.surface.webview != nil)
    [self.window makeFirstResponder:self.surface.webview];
  if (event.clickCount == 2) {
    self.surface.editorPanX = 0;
    self.surface.editorPanY = 0;
    self.surface.editorZoom = 1.0;
    apply_editor_transform(self.surface);
    if (self.surface.transformCallback)
      self.surface.transformCallback(100.0,
                                     self.surface.transformContext);
    return;
  }
  self.dragOrigin = [self convertPoint:event.locationInWindow fromView:nil];
  self.dragPan = NSMakePoint(self.surface.editorPanX,
                             self.surface.editorPanY);
  NSPoint point = self.dragOrigin;
  NSRect selectionFrame = selection_display_frame(self.surface);
  uint32_t handleEdges = selection_handle_edges(self.surface, point);
  BOOL canGesture = self.surface.editorEnabled &&
                    self.surface.selectionGestureCallback != NULL &&
                    self.surface.hasSelection &&
                    self.surface.selection.pane_index < self.surface.editorBaseRects.count;
  if (event.buttonNumber == 0 && self.surface.selectionHitTestingEnabled) {
    ScreenwidePreviewSelection target;
    uint8_t sharedHandle = 0;
    BOOL hasSharedHit =
        shared_selection_hit(self.surface, point, &target, &sharedHandle);
    BOOL inactiveFrame = hasSharedHit &&
        target.layer_id == ScreenwideFrameLayerId &&
        (!self.surface.hasSelection ||
         self.surface.selection.pane_index != target.pane_index ||
         self.surface.selection.layer_id != target.layer_id);
    if (inactiveFrame) {
      self.surface.hasSelection = YES;
      self.surface.selection = target;
      self.selectionDragActive = NO;
      self.panning = NO;
      clear_selection_snap_guides(self.surface);
      if (self.surface.selectionCallback != NULL)
        self.surface.selectionCallback((int32_t)target.pane_index,
                                       self.surface.selectionContext);
      redraw_selection(self.surface);
      invalidate_selection_cursor_rects(self.surface);
      set_selection_cursor([NSCursor arrowCursor]);
      return;
    }
    if (hasSharedHit &&
        !(sharedHandle == 0 &&
          target.layer_id == ScreenwideFrameLayerId)) {
      BOOL changed = !self.surface.hasSelection ||
                     self.surface.selection.pane_index != target.pane_index ||
                     self.surface.selection.layer_id != target.layer_id;
      self.surface.hasSelection = YES;
      self.surface.selection = target;
      self.selectionDragActive = YES;
      self.selectionDragEdges = shared_handle_edges(sharedHandle);
      self.panning = NO;
      self.selectionDragOrigin = point;
      self.selectionDragStart = target;
      clear_selection_snap_guides(self.surface);
      if (sharedHandle == 9) {
        self.selectionDragOperation = selection_is_frame(self.surface) ? 4 : 2;
        emit_selection_gesture(self.surface, 0, self.selectionDragOperation, 0,
                               target.radius_percent, 0.0, 0.0);
        set_selection_cursor(selection_resize_cursor(1 | 4));
      } else if (self.selectionDragEdges != 0) {
        self.selectionDragOperation = selection_is_frame(self.surface)
            ? 3 : target.crop_mode != 0 ? 6 : 1;
        if (self.selectionDragOperation == 3) {
          self.selectionFrameDragStart =
              self.surface.editorBaseRects[target.pane_index].rectValue;
          self.selectionFrameZoomStart = self.surface.editorZoom;
          self.selectionFramePanStart =
              NSMakePoint(self.surface.editorPanX, self.surface.editorPanY);
          self.selectionFramePaneStarts = [self.surface.editorBaseRects copy];
          begin_workspace_frame_resize(self.surface);
        }
        self.selectionDragCentered =
            (event.modifierFlags & NSEventModifierFlagOption) != 0;
        emit_selection_gesture(self.surface, 0, self.selectionDragOperation,
                               self.selectionDragEdges, 1.0, 0.0, 0.0);
        set_selection_cursor(selection_resize_cursor(self.selectionDragEdges));
      } else {
        self.selectionDragOperation = target.crop_mode != 0 ? 5 : 0;
        if (self.selectionDragOperation == 0) [self beginWorkspaceMove];
        emit_selection_gesture(self.surface, 0, self.selectionDragOperation,
                               0, 1.0, 0.0, 0.0);
        set_selection_move_cursor();
      }
      if (changed && self.surface.selectionCallback != NULL) {
        int32_t selectedIndex = selection_is_frame(self.surface)
            ? (int32_t)target.pane_index
            : (int32_t)target.layer_id;
        self.surface.selectionCallback(selectedIndex,
                                       self.surface.selectionContext);
      }
      redraw_selection(self.surface);
      invalidate_selection_cursor_rects(self.surface);
      return;
    }
  }
  if (canGesture && selection_radius_hit(self.surface, point) &&
      event.buttonNumber == 0) {
    clear_selection_snap_guides(self.surface);
    self.selectionDragActive = YES;
    self.selectionDragOperation = selection_is_frame(self.surface) ? 4 : 2;
    self.selectionDragEdges = 0;
    self.panning = NO;
    self.selectionDragOrigin = point;
    self.selectionDragStart = self.surface.selection;
    emit_selection_gesture(self.surface, 0, self.selectionDragOperation, 0,
                           self.selectionDragStart.radius_percent, 0.0, 0.0);
    set_selection_cursor(selection_resize_cursor(1 | 4));
    return;
  }
  if (canGesture && handleEdges != 0 && event.buttonNumber == 0) {
    clear_selection_snap_guides(self.surface);
    self.selectionDragActive = YES;
    self.selectionDragOperation = selection_is_frame(self.surface)
        ? 3 : self.surface.selection.crop_mode != 0 ? 6 : 1;
    self.selectionDragEdges = handleEdges;
    self.panning = NO;
    self.selectionDragOrigin = point;
    self.selectionDragStart = self.surface.selection;
    if (self.selectionDragOperation == 3)
      self.selectionFrameDragStart =
          self.surface.editorBaseRects[self.selectionDragStart.pane_index].rectValue;
    if (self.selectionDragOperation == 3) {
      self.selectionFrameZoomStart = self.surface.editorZoom;
      self.selectionFramePanStart =
          NSMakePoint(self.surface.editorPanX, self.surface.editorPanY);
      self.selectionFramePaneStarts = [self.surface.editorBaseRects copy];
      begin_workspace_frame_resize(self.surface);
    }
    self.selectionDragCentered =
        (event.modifierFlags & NSEventModifierFlagOption) != 0;
    emit_selection_gesture(self.surface, 0, self.selectionDragOperation,
                           handleEdges, 1.0, 0.0, 0.0);
    set_selection_cursor(selection_resize_cursor(handleEdges));
    return;
  }
  if (event.buttonNumber == 0 && self.surface.selectionHitTestingEnabled) {
    ScreenwidePreviewSelection target;
    if (selection_target_at_point(self.surface, point, &target)) {
      BOOL changed = !self.surface.hasSelection ||
                     self.surface.selection.pane_index != target.pane_index ||
                     self.surface.selection.layer_id != target.layer_id;
      if (target.layer_id == ScreenwideFrameLayerId) {
        self.surface.hasSelection = YES;
        self.surface.selection = target;
        self.selectionDragActive = NO;
        self.panning = NO;
        clear_selection_snap_guides(self.surface);
        if (changed && self.surface.selectionCallback != NULL)
          self.surface.selectionCallback((int32_t)target.pane_index,
                                         self.surface.selectionContext);
        redraw_selection(self.surface);
        invalidate_selection_cursor_rects(self.surface);
        set_selection_cursor([NSCursor arrowCursor]);
        return;
      }
      // React updates target hit regions asynchronously. When this is already
      // the selected pane, its native selection is the freshest geometry (for
      // example immediately after a resize). Do not replace it with a stale
      // target rectangle at the start of the next move.
      ScreenwidePreviewSelection dragTarget =
          !changed ? self.surface.selection : target;
      self.surface.hasSelection = YES;
      self.surface.selection = dragTarget;
      self.selectionDragActive = YES;
      self.selectionDragOperation = dragTarget.crop_mode != 0 ? 5 : 0;
      self.selectionDragEdges = 0;
      self.panning = NO;
      self.selectionDragOrigin = point;
      self.selectionDragStart = dragTarget;
      if (self.selectionDragOperation == 0) [self beginWorkspaceMove];
      clear_selection_snap_guides(self.surface);
      if (changed && self.surface.selectionCallback != NULL)
        self.surface.selectionCallback((int32_t)target.layer_id,
                                       self.surface.selectionContext);
      redraw_selection(self.surface);
      invalidate_selection_cursor_rects(self.surface);
      emit_selection_gesture(self.surface, 0, self.selectionDragOperation,
                             0, 1.0, 0.0, 0.0);
      set_selection_move_cursor();
      return;
    }
    self.selectionDragActive = NO;
    self.panning = YES;
    set_selection_cursor([NSCursor closedHandCursor]);
    return;
  }
  BOOL isSelectionBody = canGesture &&
                         !selection_is_frame(self.surface) &&
                         NSPointInRect(point, selectionFrame);
  if (isSelectionBody && event.buttonNumber == 0) {
    self.selectionDragActive = YES;
    self.selectionDragOperation = self.surface.selection.crop_mode != 0 ? 5 : 0;
    self.selectionDragEdges = 0;
    self.panning = NO;
    self.selectionDragOrigin = point;
    self.selectionDragStart = self.surface.selection;
    if (self.selectionDragOperation == 0) [self beginWorkspaceMove];
    clear_selection_snap_guides(self.surface);
    emit_selection_gesture(self.surface, 0, self.selectionDragOperation,
                           0, 1.0, 0.0, 0.0);
    set_selection_move_cursor();
  } else {
    self.selectionDragActive = NO;
    self.panning = YES;
    set_selection_cursor([NSCursor closedHandCursor]);
  }
}
- (void)mouseDragged:(NSEvent *)event {
  NSPoint point = [self convertPoint:event.locationInWindow fromView:nil];
  if (self.selectionDragActive) {
    NSPoint delta = NSMakePoint(point.x - self.selectionDragOrigin.x,
                                point.y - self.selectionDragOrigin.y);
    if (self.selectionDragOperation == 0 &&
        (event.modifierFlags & NSEventModifierFlagShift)) {
      if (fabs(delta.x) >= fabs(delta.y)) delta.y = 0;
      else delta.x = 0;
    }
    NSRect pane = self.surface.editorBaseRects[
        self.selectionDragStart.pane_index].rectValue;
    if (self.selectionDragOperation == 5) {
      NSRect pane = self.surface.editorBaseRects[
          self.selectionDragStart.pane_index].rectValue;
      double dx = delta.x /
          MAX(pane.size.width * self.surface.editorZoom, 1.0);
      double dy = delta.y /
          MAX(pane.size.height * self.surface.editorZoom, 1.0);
      ScreenwidePreviewSelection moved = self.selectionDragStart;
      moved.x = fmin(self.selectionDragStart.image_x +
                         self.selectionDragStart.image_width - moved.width,
                     fmax(self.selectionDragStart.image_x,
                          self.selectionDragStart.x + dx));
      moved.y = fmin(self.selectionDragStart.image_y +
                         self.selectionDragStart.image_height - moved.height,
                     fmax(self.selectionDragStart.image_y,
                          self.selectionDragStart.y + dy));
      self.surface.selection = moved;
      redraw_selection(self.surface);
      emit_selection_gesture(self.surface, 1, 5, 0, 1.0,
                             moved.x - self.selectionDragStart.x,
                             moved.y - self.selectionDragStart.y);
    } else if (self.selectionDragOperation == 6) {
      NSRect pane = self.surface.editorBaseRects[
          self.selectionDragStart.pane_index].rectValue;
      double dx = delta.x /
          MAX(pane.size.width * self.surface.editorZoom, 1.0);
      double dy = delta.y /
          MAX(pane.size.height * self.surface.editorZoom, 1.0);
      ScreenwidePreviewSelection start = self.selectionDragStart;
      double left = start.x, top = start.y;
      double right = start.x + start.width;
      double bottom = start.y + start.height;
      double minimumWidth = 36.0 /
          MAX(pane.size.width * self.surface.editorZoom, 1.0);
      double minimumHeight = 36.0 /
          MAX(pane.size.height * self.surface.editorZoom, 1.0);
      if (self.selectionDragEdges & 1)
        left = fmin(right - minimumWidth,
                    fmax(start.image_x, start.x + dx));
      if (self.selectionDragEdges & 2)
        right = fmax(left + minimumWidth,
                     fmin(start.image_x + start.image_width,
                          start.x + start.width + dx));
      if (self.selectionDragEdges & 4)
        top = fmin(bottom - minimumHeight,
                   fmax(start.image_y, start.y + dy));
      if (self.selectionDragEdges & 8)
        bottom = fmax(top + minimumHeight,
                      fmin(start.image_y + start.image_height,
                           start.y + start.height + dy));
      ScreenwidePreviewSelection cropped = start;
      cropped.x = left;
      cropped.y = top;
      cropped.width = right - left;
      cropped.height = bottom - top;
      self.surface.selection = cropped;
      NSRect cropFrame = selection_display_frame_for(self.surface, cropped);
      NSPoint handlePoint = NSMakePoint(
          (self.selectionDragEdges & 1) ? NSMinX(cropFrame)
          : (self.selectionDragEdges & 2) ? NSMaxX(cropFrame)
                                          : point.x,
          (self.selectionDragEdges & 4) ? NSMinY(cropFrame)
          : (self.selectionDragEdges & 8) ? NSMaxY(cropFrame)
                                          : point.y);
      update_crop_magnifier(self.surface, handlePoint,
                            self.selectionDragEdges);
      redraw_selection(self.surface);
      // Crop resize emits the effective moved edge coordinates. The shared
      // semantic mirror can reproduce this exact rectangle without an
      // independent pointer-to-layout calculation.
      double effectiveX = (self.selectionDragEdges & 1)
          ? left - start.x
          : (self.selectionDragEdges & 2)
              ? right - (start.x + start.width) : 0.0;
      double effectiveY = (self.selectionDragEdges & 4)
          ? top - start.y
          : (self.selectionDragEdges & 8)
              ? bottom - (start.y + start.height) : 0.0;
      emit_selection_gesture(self.surface, 1, 6,
                             self.selectionDragEdges, 1.0,
                             effectiveX, effectiveY);
    } else if (self.selectionDragOperation == 2 || self.selectionDragOperation == 4) {
      NSRect frame = selection_display_frame_for(self.surface,
                                                  self.selectionDragStart);
      double shortest = MAX(MIN(frame.size.width, frame.size.height), 1.0);
      double radius = (((point.x - NSMinX(frame)) +
                        (point.y - NSMinY(frame))) / 2.0 - 10.0) / 0.55;
      double radiusPercent = fmin(50.0, fmax(0.0, radius * 100.0 / shortest));
      ScreenwidePreviewSelection rounded = self.surface.selection;
      rounded.radius_percent = radiusPercent;
      self.surface.selection = rounded;
      BOOL directlyEditsWorkspaceLayer =
          self.selectionDragOperation == 4 ||
          (self.selectionDragOperation == 2 &&
           self.selectionDragStart.layer_id ==
               self.selectionDragStart.pane_index);
      if (directlyEditsWorkspaceLayer &&
          self.surface.workspaceExplicitPlacements) {
        [self.surface.workspaceLock lock];
        screenwide_gpu_still_presenter_update_workspace_selected_radius(
            self.surface.views[0].compositor,
            self.selectionDragStart.pane_index, radiusPercent);
        [self.surface.workspaceLock unlock];
        redraw_workspace(self.surface);
      }
      redraw_selection(self.surface);
      emit_selection_gesture(self.surface, 1, self.selectionDragOperation,
                             0, radiusPercent, 0.0, 0.0);
    } else if (self.selectionDragOperation == 3) {
      NSRect start = self.selectionFrameDragStart;
      uint32_t edges = self.selectionDragEdges;
      BOOL centered = (event.modifierFlags & NSEventModifierFlagOption) != 0;
      self.selectionDragCentered = centered;
      // editorBaseRects are pre-zoom workspace coordinates while AppKit mouse
      // events are display points. Use one inverse transform for native frame
      // geometry and the semantic canvas delta so OSC and pixels cannot drift.
      double inverseZoom = 1.0 / MAX(self.selectionFrameZoomStart, 0.000001);
      NSPoint workspaceDelta = NSMakePoint(delta.x * inverseZoom,
                                           delta.y * inverseZoom);
      double left = NSMinX(start), right = NSMaxX(start);
      double top = NSMinY(start), bottom = NSMaxY(start);
      double minimum = 36.0;
      if (edges & 1) {
        double movement = MIN(centered ? (start.size.width - minimum) / 2.0
                                       : start.size.width - minimum,
                              workspaceDelta.x);
        left += movement;
        if (centered) right -= movement;
      } else if (edges & 2) {
        double movement = MAX(centered ? -(start.size.width - minimum) / 2.0
                                       : minimum - start.size.width,
                              workspaceDelta.x);
        right += movement;
        if (centered) left -= movement;
      }
      if (edges & 4) {
        double movement = MIN(centered ? (start.size.height - minimum) / 2.0
                                       : start.size.height - minimum,
                              workspaceDelta.y);
        top += movement;
        if (centered) bottom -= movement;
      } else if (edges & 8) {
        double movement = MAX(centered ? -(start.size.height - minimum) / 2.0
                                       : minimum - start.size.height,
                              workspaceDelta.y);
        bottom += movement;
        if (centered) top -= movement;
      }
      NSRect resizedFrame = NSMakeRect(left, top, right - left, bottom - top);
      // A screenshot workspace is composed from one full-canvas Metal pane
      // per source. Frame owns the workspace, so resize every coincident pane
      // together instead of stretching only the selected source layer.
      if (self.surface.workspaceExplicitPlacements) {
        double originX = (resizedFrame.origin.x - start.origin.x) /
            MAX(start.size.width, 1.0);
        double originY = (resizedFrame.origin.y - start.origin.y) /
            MAX(start.size.height, 1.0);
        double width = resizedFrame.size.width / MAX(start.size.width, 1.0);
        double height = resizedFrame.size.height / MAX(start.size.height, 1.0);
        [self.surface.workspaceLock lock];
        screenwide_gpu_still_presenter_update_workspace_selected_resize(
            self.surface.views[0].compositor,
            self.selectionDragStart.pane_index,
            originX, originY, width, height);
        [self.surface.workspaceLock unlock];
        reflow_recording_workspace_panes(
            self.surface, self.selectionFramePaneStarts,
            self.selectionDragStart.pane_index, resizedFrame);
        rebase_recording_workspace_fit(
            self.surface, self.selectionFramePaneStarts,
            self.selectionFrameZoomStart,
            self.selectionFramePanStart);
      } else {
        update_workspace_frame_resize(self.surface, start, resizedFrame);
        NSRect displayedFrame = editor_frame_with_transform(
            self.surface, resizedFrame, self.selectionFrameZoomStart,
            self.selectionFramePanStart);
        NSRect fitFrame = rebase_workspace_fit(self.surface, displayedFrame);
        for (NSUInteger index = 0; index < self.surface.editorBaseRects.count; index++)
          self.surface.editorBaseRects[index] = [NSValue valueWithRect:fitFrame];
      }
      apply_editor_transform(self.surface);
      if (self.surface.transformCallback)
        self.surface.transformCallback(self.surface.editorZoom * 100.0,
                                       self.surface.transformContext);
      uint32_t emittedEdges = edges |
          (centered ? ScreenwideCenteredResizeEdge : 0);
      emit_selection_gesture(
          self.surface, 1, 3, emittedEdges, 1.0,
          workspaceDelta.x / MAX(start.size.width, 1.0),
          workspaceDelta.y / MAX(start.size.height, 1.0));
    } else if (self.selectionDragOperation == 1) {
      NSRect pane = self.surface.editorBaseRects[self.selectionDragStart.pane_index].rectValue;
      double dx = delta.x / MAX(pane.size.width * self.surface.editorZoom, 1.0);
      double dy = delta.y / MAX(pane.size.height * self.surface.editorZoom, 1.0);
      uint32_t edges = self.selectionDragEdges;
      ScreenwidePreviewSelection start = self.selectionDragStart;
      double x = start.x, y = start.y, width = start.width, height = start.height;
      BOOL centered = (event.modifierFlags & NSEventModifierFlagOption) != 0;
      double anchorX = centered ? start.x + start.width / 2.0
                                : (edges & 1) ? start.x + start.width
                                              : (edges & 2) ? start.x : start.x + start.width / 2.0;
      double anchorY = centered ? start.y + start.height / 2.0
                                : (edges & 4) ? start.y + start.height
                                              : (edges & 8) ? start.y : start.y + start.height / 2.0;
      double handleX = (edges & 1) ? start.x : (edges & 2) ? start.x + start.width
                                                            : start.x + start.width / 2.0;
      double handleY = (edges & 4) ? start.y : (edges & 8) ? start.y + start.height
                                                            : start.y + start.height / 2.0;
      double vectorX = handleX - anchorX;
      double vectorY = handleY - anchorY;
      double denominator = vectorX * vectorX + vectorY * vectorY;
      double scale = denominator > 0.0
          ? ((dx + handleX - anchorX) * vectorX +
             (dy + handleY - anchorY) * vectorY) / denominator
          : 1.0;
      double minimumWidthScale = 36.0 /
          MAX(pane.size.width * self.surface.editorZoom * start.width, 1.0);
      double minimumHeightScale = 36.0 /
          MAX(pane.size.height * self.surface.editorZoom * start.height, 1.0);
      double minimumScale = MAX(minimumWidthScale, minimumHeightScale);
      scale = fmin(8.0, fmax(minimumScale, scale));
      BOOL snapping = self.surface.selectionSnappingEnabled &&
          (event.modifierFlags & (NSEventModifierFlagCommand |
                                  NSEventModifierFlagControl)) != 0;
      if (snapping)
        scale = snap_selection_resize(
            self.surface, scale, anchorX, anchorY, vectorX, vectorY,
            edges, pane, minimumScale, 8.0);
      else
        clear_selection_snap_guides(self.surface);
      x = anchorX + (start.x - anchorX) * scale;
      y = anchorY + (start.y - anchorY) * scale;
      width = start.width * scale;
      height = start.height * scale;
      ScreenwidePreviewSelection resized = start;
      resized.x = x; resized.y = y; resized.width = width; resized.height = height;
      self.surface.selection = resized;
      apply_editor_transform(self.surface);
      emit_selection_gesture(self.surface, 1, 1, edges, scale,
                             x - start.x, y - start.y);
    } else {
      BOOL optionHeld =
          (event.modifierFlags & NSEventModifierFlagOption) != 0;
      if (self.selectionMoveAutoFitActive && !optionHeld) {
        // Releasing Option accepts the grown canvas. Rebase the remainder of
        // this mouse gesture onto that committed scene, while React/Rust keep
        // one edit-history transaction open across the checkpoint.
        self.surface.editorPanX = 0.0;
        self.surface.editorPanY = 0.0;
        apply_editor_transform(self.surface);
        if (self.surface.transformCallback)
          self.surface.transformCallback(self.surface.editorZoom * 100.0,
                                         self.surface.transformContext);
        end_workspace_frame_resize(self.surface, YES);
        self.selectionDragStart = self.surface.selection;
        self.selectionDragOrigin = point;
        self.selectionMoveDeltaX = 0.0;
        self.selectionMoveDeltaY = 0.0;
        // The committed canvas becomes the move's new starting point, so
        // Option can grow it again later in this same gesture: re-express
        // the mouse-down targets in it and re-snapshot the workspace exactly
        // as beginWorkspaceMove did at mouse-down.
        NSRect bounds = self.selectionMoveAutoFitBounds;
        if (bounds.size.width > 0.0 && bounds.size.height > 0.0) {
          NSMutableArray<NSValue *> *rebased = [NSMutableArray
              arrayWithCapacity:self.selectionMoveTargetsStart.count];
          for (NSValue *value in self.selectionMoveTargetsStart) {
            ScreenwidePreviewSelection target;
            [value getValue:&target size:sizeof(target)];
            target.x = (target.x - bounds.origin.x) / bounds.size.width;
            target.y = (target.y - bounds.origin.y) / bounds.size.height;
            target.width /= bounds.size.width;
            target.height /= bounds.size.height;
            [rebased addObject:[NSValue valueWithBytes:&target
                                              objCType:@encode(ScreenwidePreviewSelection)]];
          }
          self.selectionMoveTargetsStart = rebased;
        }
        self.selectionMoveAutoFitBounds = NSZeroRect;
        self.selectionMoveAutoFitActive = NO;
        self.selectionMoveZoomStart = self.surface.editorZoom;
        self.selectionMovePanStart =
            NSMakePoint(self.surface.editorPanX, self.surface.editorPanY);
        self.selectionFramePaneStarts = [self.surface.editorBaseRects copy];
        NSUInteger movePaneIndex = self.selectionDragStart.pane_index;
        self.selectionMoveFrameStart =
            movePaneIndex < self.surface.editorBaseRects.count
                ? self.surface.editorBaseRects[movePaneIndex].rectValue
                : NSZeroRect;
        begin_workspace_frame_resize(self.surface);
        self.selectionDragEdges = ScreenwideAutoFitCommitEdge;
        clear_selection_snap_guides(self.surface);
        emit_selection_gesture(self.surface, 1, 0,
                               ScreenwideAutoFitCommitEdge, 1.0, 0.0, 0.0);
        self.selectionDragEdges = 0;
        return;
      }
      NSRect movePane = NSIsEmptyRect(self.selectionMoveFrameStart)
          ? pane : self.selectionMoveFrameStart;
      double moveDeltaX = delta.x /
          MAX(movePane.size.width * self.selectionMoveZoomStart, 1.0);
      double moveDeltaY = delta.y /
          MAX(movePane.size.height * self.selectionMoveZoomStart, 1.0);
      double x = self.selectionDragStart.x + moveDeltaX;
      double y = self.selectionDragStart.y + moveDeltaY;
      BOOL snapping = self.surface.selectionSnappingEnabled &&
          (event.modifierFlags & (NSEventModifierFlagCommand |
                                  NSEventModifierFlagControl)) != 0;
      if (snapping) snap_selection_move(self.surface, &x, &y);
      else clear_selection_snap_guides(self.surface);
      // Auto-fit renormalizes the live OSC into each enlarged canvas. Always
      // derive the next sample from mouse-down geometry; reusing the already
      // renormalized width compounds that normalization and collapses the OSC.
      ScreenwidePreviewSelection moved = self.selectionDragStart;
      moved.x = x;
      moved.y = y;
      self.selectionMoveDeltaX = x - self.selectionDragStart.x;
      self.selectionMoveDeltaY = y - self.selectionDragStart.y;
      BOOL autoFit = self.surface.workspaceMode &&
          optionHeld &&
          !NSIsEmptyRect(self.selectionMoveFrameStart);
      self.selectionDragEdges = autoFit ? ScreenwideAutoFitMoveEdge : 0;
      if (autoFit) {
        self.selectionMoveAutoFitActive = YES;
        NSRect bounds = auto_fit_selection_bounds(
            self.surface, self.selectionMoveTargetsStart, moved);
        self.selectionMoveAutoFitBounds = bounds;
        NSRect start = self.selectionMoveFrameStart;
        NSRect resized = NSMakeRect(
            start.origin.x + bounds.origin.x * start.size.width,
            start.origin.y + bounds.origin.y * start.size.height,
            bounds.size.width * start.size.width,
            bounds.size.height * start.size.height);
        if (self.surface.workspaceExplicitPlacements) {
          double originX = (resized.origin.x - start.origin.x) /
              MAX(start.size.width, 1.0);
          double originY = (resized.origin.y - start.origin.y) /
              MAX(start.size.height, 1.0);
          double width = resized.size.width / MAX(start.size.width, 1.0);
          double height = resized.size.height / MAX(start.size.height, 1.0);
          [self.surface.workspaceLock lock];
          screenwide_gpu_still_presenter_update_recording_auto_fit_move(
              self.surface.views[0].compositor,
              self.selectionDragStart.layer_id,
              self.selectionMoveDeltaX, self.selectionMoveDeltaY,
              originX, originY, width, height);
          [self.surface.workspaceLock unlock];
          reflow_recording_workspace_panes(
              self.surface, self.selectionFramePaneStarts,
              self.selectionDragStart.pane_index, resized);
          rebase_recording_workspace_fit(
              self.surface, self.selectionFramePaneStarts,
              self.selectionMoveZoomStart, self.selectionMovePanStart);
        } else {
          update_workspace_auto_fit_move(
              self.surface, self.selectionDragStart.layer_id,
              self.selectionMoveDeltaX, self.selectionMoveDeltaY,
              start, resized);
          NSRect displayed = editor_frame_with_transform(
              self.surface, resized, self.selectionMoveZoomStart,
              self.selectionMovePanStart);
          NSRect fit = rebase_workspace_fit(self.surface, displayed);
          for (NSUInteger index = 0;
               index < self.surface.editorBaseRects.count; index++)
            self.surface.editorBaseRects[index] = [NSValue valueWithRect:fit];
        }
        moved.x = (moved.x - bounds.origin.x) / bounds.size.width;
        moved.y = (moved.y - bounds.origin.y) / bounds.size.height;
        moved.width /= bounds.size.width;
        moved.height /= bounds.size.height;
      } else if (self.selectionMoveAutoFitActive &&
                 !NSIsEmptyRect(self.selectionMoveFrameStart)) {
        self.selectionMoveAutoFitActive = NO;
        update_workspace_frame_resize(
            self.surface, self.selectionMoveFrameStart,
            self.selectionMoveFrameStart);
        for (NSUInteger index = 0;
             index < self.surface.editorBaseRects.count; index++)
          self.surface.editorBaseRects[index] =
              [NSValue valueWithRect:self.selectionMoveFrameStart];
        self.surface.editorZoom = self.selectionMoveZoomStart;
        self.surface.editorPanX = self.selectionMovePanStart.x;
        self.surface.editorPanY = self.selectionMovePanStart.y;
      }
      self.surface.selection = moved;
      apply_editor_transform(self.surface);
      if (autoFit && self.surface.transformCallback)
        self.surface.transformCallback(self.surface.editorZoom * 100.0,
                                       self.surface.transformContext);
      emit_selection_gesture(self.surface, 1, 0, self.selectionDragEdges, 1.0,
                             self.selectionMoveDeltaX,
                             self.selectionMoveDeltaY);
    }
    return;
  }
  self.surface.editorPanX = self.dragPan.x + point.x - self.dragOrigin.x;
  self.surface.editorPanY = self.dragPan.y + point.y - self.dragOrigin.y;
  apply_editor_transform(self.surface);
}
- (void)mouseUp:(NSEvent *)event {
  BOOL hadSnapGuides = self.surface.hasSelectionSnapGuideX ||
                       self.surface.hasSelectionSnapGuideY;
  BOOL hadMagnifier = self.surface.workspaceMagnifier.active != 0;
  if (self.selectionDragActive) {
    // AppKit can deliver mouse-up at a newer location than the last drag
    // event. Apply that final Frame sample before committing so its OSC,
    // pane geometry and semantic payload share the same endpoint.
    if (self.selectionDragOperation == 3 ||
        self.selectionDragOperation == 5 ||
        self.selectionDragOperation == 6)
      [self mouseDragged:event];
    NSPoint point = [self convertPoint:event.locationInWindow fromView:nil];
    double scale = (self.selectionDragOperation == 2 ||
                    self.selectionDragOperation == 4)
                       ? self.surface.selection.radius_percent
                       : self.selectionDragOperation == 1 &&
                           self.selectionDragStart.width > 0.0
                       ? self.surface.selection.width /
                             self.selectionDragStart.width
                       : 1.0;
    uint32_t edges = self.selectionDragEdges;
    double deltaX = self.surface.selection.x - self.selectionDragStart.x;
    double deltaY = self.surface.selection.y - self.selectionDragStart.y;
    if (self.selectionDragOperation == 6) {
      if (edges & 2)
        deltaX = self.surface.selection.x + self.surface.selection.width -
            (self.selectionDragStart.x + self.selectionDragStart.width);
      if (edges & 8)
        deltaY = self.surface.selection.y + self.surface.selection.height -
            (self.selectionDragStart.y + self.selectionDragStart.height);
    }
    if (self.selectionDragOperation == 0 &&
        !NSIsEmptyRect(self.selectionMoveFrameStart)) {
      deltaX = self.selectionMoveDeltaX;
      deltaY = self.selectionMoveDeltaY;
    }
    if (self.selectionDragOperation == 3) {
      NSPoint delta = NSMakePoint(point.x - self.selectionDragOrigin.x,
                                  point.y - self.selectionDragOrigin.y);
      edges |= self.selectionDragCentered ? ScreenwideCenteredResizeEdge : 0;
      double inverseZoom = 1.0 / MAX(self.selectionFrameZoomStart, 0.000001);
      deltaX = delta.x * inverseZoom /
          MAX(self.selectionFrameDragStart.size.width, 1.0);
      deltaY = delta.y * inverseZoom /
          MAX(self.selectionFrameDragStart.size.height, 1.0);
      self.surface.editorPanX = 0.0;
      self.surface.editorPanY = 0.0;
      apply_editor_transform(self.surface);
    }
    emit_selection_gesture(self.surface, 2, self.selectionDragOperation,
                           edges, scale, deltaX, deltaY);
    if (self.selectionDragOperation == 3)
      end_workspace_frame_resize(self.surface, YES);
    else if (self.selectionDragOperation == 0 &&
             !NSIsEmptyRect(self.selectionMoveFrameStart))
      end_workspace_frame_resize(self.surface, YES);
  }
  clear_selection_snap_guides(self.surface);
  self.selectionDragActive = NO;
  self.selectionDragOperation = 0;
  self.selectionDragEdges = 0;
  self.selectionMoveFrameStart = NSZeroRect;
  self.selectionMoveAutoFitActive = NO;
  self.selectionMoveAutoFitBounds = NSZeroRect;
  self.selectionMoveTargetsStart = nil;
  self.selectionFramePaneStarts = nil;
  self.panning = NO;
  ScreenwideWorkspaceMagnifier clearedMagnifier = self.surface.workspaceMagnifier;
  clearedMagnifier.active = 0;
  self.surface.workspaceMagnifier = clearedMagnifier;
  if (hadSnapGuides || hadMagnifier) redraw_selection(self.surface);
  NSPoint point = [self convertPoint:event.locationInWindow fromView:nil];
  set_selection_cursor_at_point(self.surface, point);
}
- (void)rightMouseUp:(NSEvent *)event { [self mouseUp:event]; }
- (void)otherMouseUp:(NSEvent *)event { [self mouseUp:event]; }
- (void)otherMouseDown:(NSEvent *)event { [self mouseDown:event]; }
- (void)otherMouseDragged:(NSEvent *)event { [self mouseDragged:event]; }
- (void)scrollWheel:(NSEvent *)event {
  if (event.modifierFlags & NSEventModifierFlagControl) {
    NSPoint point = [self convertPoint:event.locationInWindow fromView:nil];
    set_editor_zoom(self.surface,
                    self.surface.editorZoom * exp(-event.scrollingDeltaY * 0.01),
                    point);
  } else {
    self.surface.editorPanX += event.scrollingDeltaX;
    self.surface.editorPanY += event.scrollingDeltaY;
    apply_editor_transform(self.surface);
  }
}
- (void)magnifyWithEvent:(NSEvent *)event {
  NSPoint point = [self convertPoint:event.locationInWindow fromView:nil];
  set_editor_zoom(self.surface,
                  self.surface.editorZoom * (1.0 + event.magnification), point);
}
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
                                   constant float4 &magnifier_box [[buffer(1)]]) {
  if (magnifier_box.z > 0.0) {
    float2 half_size = magnifier_box.zw * 0.5;
    float2 local = abs(in.position.xy - (magnifier_box.xy + half_size)) -
                   (half_size - 4.0);
    float distance = length(max(local, 0.0)) +
                     min(max(local.x, local.y), 0.0) - 4.0;
    if (distance <= 0.0) discard_fragment();
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
static void on_main_async(dispatch_block_t block) {
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
static void present_in_transaction(ScreenwidePreviewSurface *surface,
                                   ScreenwidePreviewView *view,
                                   id<MTLCommandBuffer> command,
                                   id<CAMetalDrawable> drawable) {
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

static void update_crop_magnifier(ScreenwidePreviewSurface *surface,
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

static void begin_workspace_frame_resize(ScreenwidePreviewSurface *surface) {
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

static void update_workspace_frame_resize(
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

static BOOL update_workspace_auto_fit_move(
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

static void end_workspace_frame_resize(
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

static void redraw_workspace(ScreenwidePreviewSurface *surface) {
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
