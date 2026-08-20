// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#import "recording_preview_surface_macos_private.h"

#import <objc/runtime.h>
#include <math.h>

/// Native selection interaction extension point for annotation and ruler tools.
static IMP original_cursor_set = NULL;
SCREENWIDE_PREVIEW_PRIVATE NSCursor *expected_selection_cursor = nil;
static NSCursor *webkit_selection_move_cursor = nil;
SCREENWIDE_PREVIEW_PRIVATE BOOL expected_selection_move_cursor = NO;

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

SCREENWIDE_PREVIEW_PRIVATE void install_native_cursor_guard(void) {
  static dispatch_once_t once;
  dispatch_once(&once, ^{
    Method method = class_getInstanceMethod(NSCursor.class, @selector(set));
    original_cursor_set = method_setImplementation(
        method, (IMP)guarded_cursor_set);
  });
}

SCREENWIDE_PREVIEW_PRIVATE NSRect selection_display_frame_for(ScreenwidePreviewSurface *surface,
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

SCREENWIDE_PREVIEW_PRIVATE NSRect selection_display_frame(ScreenwidePreviewSurface *surface) {
  if (!surface.hasSelection) return NSZeroRect;
  return selection_display_frame_for(surface, surface.selection);
}

SCREENWIDE_PREVIEW_PRIVATE NSRect auto_fit_selection_bounds(
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

SCREENWIDE_PREVIEW_PRIVATE BOOL selection_is_frame(ScreenwidePreviewSurface *surface) {
  return surface.hasSelection &&
         surface.selection.layer_id == ScreenwideFrameLayerId;
}

SCREENWIDE_PREVIEW_PRIVATE BOOL selection_target_at_point(ScreenwidePreviewSurface *surface,
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

SCREENWIDE_PREVIEW_PRIVATE BOOL shared_selection_hit(ScreenwidePreviewSurface *surface,
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

SCREENWIDE_PREVIEW_PRIVATE uint32_t shared_handle_edges(uint8_t handle) {
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

SCREENWIDE_PREVIEW_PRIVATE void emit_selection_gesture(ScreenwidePreviewSurface *surface,
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
SCREENWIDE_PREVIEW_PRIVATE uint32_t selection_handle_edges(ScreenwidePreviewSurface *surface,
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

SCREENWIDE_PREVIEW_PRIVATE BOOL selection_radius_hit(ScreenwidePreviewSurface *surface,
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

SCREENWIDE_PREVIEW_PRIVATE void clear_selection_snap_guides(ScreenwidePreviewSurface *surface);

SCREENWIDE_PREVIEW_PRIVATE double snap_selection_resize(ScreenwidePreviewSurface *surface,
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

SCREENWIDE_PREVIEW_PRIVATE void clear_selection_snap_guides(ScreenwidePreviewSurface *surface) {
  surface.hasSelectionSnapGuideX = NO;
  surface.hasSelectionSnapGuideY = NO;
}

SCREENWIDE_PREVIEW_PRIVATE void snap_selection_move(ScreenwidePreviewSurface *surface,
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

SCREENWIDE_PREVIEW_PRIVATE NSCursor *selection_resize_cursor(uint32_t edges) {
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

SCREENWIDE_PREVIEW_PRIVATE void set_selection_cursor(NSCursor *cursor) {
  expected_selection_move_cursor = NO;
  expected_selection_cursor = cursor;
  [cursor set];
}

SCREENWIDE_PREVIEW_PRIVATE void set_selection_move_cursor(void) {
  expected_selection_move_cursor = YES;
  expected_selection_cursor = selection_move_cursor();
  [expected_selection_cursor set];
}

SCREENWIDE_PREVIEW_PRIVATE void set_selection_cursor_at_point(ScreenwidePreviewSurface *surface,
                                          NSPoint point) {
  NSCursor *cursor = selection_cursor(surface, point);
  if (cursor == selection_move_cursor()) set_selection_move_cursor();
  else set_selection_cursor(cursor);
}

SCREENWIDE_PREVIEW_PRIVATE void invalidate_selection_cursor_rects(ScreenwidePreviewSurface *surface) {
  if (surface.interaction.window != nil)
    [surface.interaction.window invalidateCursorRectsForView:surface.interaction];
}
