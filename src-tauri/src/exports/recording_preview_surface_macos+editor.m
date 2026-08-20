// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#import "recording_preview_surface_macos_private.h"
#include <math.h>

@implementation ScreenwidePreviewInteractionView (Editor)
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
  // A stale commit flag must not outlive its gesture (see its declaration).
  self.surface.keepTransformForCommittedNaturalSize = NO;
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
            self.selectionDragStart.pane_index, radiusPercent,
            self.selectionDragOperation == 4 ? 1 : 0);
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
        // one edit-history transaction open across the checkpoint. The
        // zoom/pan already express the grown canvas pixel-for-pixel (the
        // auto-fit samples rebased them), so they are kept as they are: a
        // recentre here would yank the workspace mid-drag.
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
    if (self.selectionDragOperation == 3) {
      end_workspace_frame_resize(self.surface, YES);
    } else if (self.selectionDragOperation == 0 &&
               !NSIsEmptyRect(self.selectionMoveFrameStart)) {
      // Unlike Frame (which recentred above), an auto-fit Move leaves the
      // view where the drag put it; the layout echoing the grown canvas must
      // keep that transform rather than restore/recentre. A plain move sets
      // this too, harmlessly: its echo changes no size and clears the flag.
      self.surface.keepTransformForCommittedNaturalSize = YES;
      end_workspace_frame_resize(self.surface, YES);
    }
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

