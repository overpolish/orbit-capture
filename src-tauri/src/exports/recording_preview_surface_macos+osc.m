// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#import "recording_preview_surface_macos_private.h"

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

/// Size of the current selection in OUTPUT pixels, or NO when the workspace
/// has no pixel scale to convert with.
///
/// `workspaceNaturalWidth/Height` is the canvas size in output pixels and the
/// pane rects are pre-zoom points, so pixels-per-point is simply natural over
/// the union of the ACTIVE pane rects. That relation holds in every gesture
/// path by construction: the screenshot workspace has a single pane whose rect
/// is the canvas (and `update_workspace_frame_resize` /
/// `update_workspace_auto_fit_move` keep natural live during a drag), and the
/// recording workspace's `rebase_recording_workspace_fit` scales natural by
/// exactly the union-bounds ratio it rebases the pane rects with.
static BOOL selection_pixel_size(ScreenwidePreviewSurface *surface,
                                 double *width, double *height) {
  if (!surface.workspaceMode || !surface.hasSelection) return NO;
  if (surface.workspaceNaturalWidth <= 0.0 ||
      surface.workspaceNaturalHeight <= 0.0) return NO;
  if (surface.selection.pane_index >= surface.editorBaseRects.count) return NO;
  NSRect bounds = NSZeroRect;
  BOOL hasBounds = NO;
  for (NSNumber *value in surface.workspaceActivePaneIndices) {
    NSUInteger index = value.unsignedIntegerValue;
    if (index >= surface.editorBaseRects.count) continue;
    NSRect frame = surface.editorBaseRects[index].rectValue;
    if (NSIsEmptyRect(frame)) continue;
    bounds = hasBounds ? NSUnionRect(bounds, frame) : frame;
    hasBounds = YES;
  }
  if (!hasBounds || NSIsEmptyRect(bounds)) return NO;
  NSRect pane = surface.editorBaseRects[surface.selection.pane_index].rectValue;
  double perPointX = surface.workspaceNaturalWidth / bounds.size.width;
  double perPointY = surface.workspaceNaturalHeight / bounds.size.height;
  *width = surface.selection.width * pane.size.width * perPointX;
  *height = surface.selection.height * pane.size.height * perPointY;
  return YES;
}

static const CGFloat ScreenwideSelectionLabelFontSize = 11.0;
static const CGFloat ScreenwideSelectionLabelStroke = 2.0;

/// (Re)builds `selectionLabelTexture` for `text`. Returns NO when the bitmap
/// could not be produced, in which case no label must be drawn.
///
/// Rasterised exactly like Keyframeless's OSC label: a monospaced string drawn
/// twice into a premultiplied sRGB bitmap - a stroked halo pass first, then the
/// fill on top - so the readout stays legible over any pane content without a
/// backing plate. The colours mirror the OSC palette in `selection_fragment`.
static BOOL update_selection_label(ScreenwidePreviewSurface *surface,
                                   NSString *text, CGFloat scale,
                                   uint32_t lightMode) {
  if (surface.device == nil || text.length == 0) return NO;
  if (surface.selectionLabelTexture != nil &&
      surface.selectionLabelScale == scale &&
      surface.selectionLabelLightMode == lightMode &&
      [surface.selectionLabelText isEqualToString:text])
    return YES;

  NSColor *fill = lightMode != 0
      ? [NSColor colorWithSRGBRed:0.12 green:0.12 blue:0.12 alpha:1.0]
      : [NSColor colorWithSRGBRed:1.0 green:1.0 blue:1.0 alpha:1.0];
  NSColor *halo = lightMode != 0
      ? [NSColor colorWithSRGBRed:1.0 green:1.0 blue:1.0 alpha:1.0]
      : [NSColor colorWithSRGBRed:0.0 green:0.0 blue:0.0 alpha:0.8];
  NSFont *font = [NSFont monospacedSystemFontOfSize:ScreenwideSelectionLabelFontSize
                                             weight:NSFontWeightMedium];
  // A positive stroke width strokes the glyph without filling it, so this pass
  // lays down only the outline the fill pass then sits inside.
  NSDictionary *strokeAttributes = @{
    NSFontAttributeName : font,
    NSForegroundColorAttributeName : halo,
    NSStrokeColorAttributeName : halo,
    NSStrokeWidthAttributeName :
        @(ScreenwideSelectionLabelStroke / ScreenwideSelectionLabelFontSize * 100.0),
  };
  NSDictionary *fillAttributes = @{
    NSFontAttributeName : font,
    NSForegroundColorAttributeName : fill,
  };
  NSSize textSize = [text sizeWithAttributes:fillAttributes];
  // The inset leaves room for the halo, which spills outside the glyph box.
  NSInteger pointWidth = (NSInteger)ceil(textSize.width) + 4;
  NSInteger pointHeight = (NSInteger)ceil(textSize.height) + 2;
  NSInteger pixelWidth = (NSInteger)MAX(round(pointWidth * scale), 1.0);
  NSInteger pixelHeight = (NSInteger)MAX(round(pointHeight * scale), 1.0);

  CGColorSpaceRef space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
  CGContextRef context = CGBitmapContextCreate(
      NULL, (size_t)pixelWidth, (size_t)pixelHeight, 8, (size_t)pixelWidth * 4,
      space,
      (CGBitmapInfo)kCGImageAlphaPremultipliedLast | kCGBitmapByteOrder32Big);
  CGColorSpaceRelease(space);
  if (context == NULL) return NO;
  CGContextScaleCTM(context, scale, scale);
  NSGraphicsContext *graphics =
      [NSGraphicsContext graphicsContextWithCGContext:context flipped:NO];
  [NSGraphicsContext saveGraphicsState];
  [NSGraphicsContext setCurrentContext:graphics];
  [text drawAtPoint:NSMakePoint(2.0, 1.0) withAttributes:strokeAttributes];
  [text drawAtPoint:NSMakePoint(2.0, 1.0) withAttributes:fillAttributes];
  [NSGraphicsContext restoreGraphicsState];

  MTLTextureDescriptor *descriptor = [MTLTextureDescriptor
      texture2DDescriptorWithPixelFormat:MTLPixelFormatRGBA8Unorm
                                   width:(NSUInteger)pixelWidth
                                  height:(NSUInteger)pixelHeight
                               mipmapped:NO];
  descriptor.usage = MTLTextureUsageShaderRead;
  id<MTLTexture> texture = [surface.device newTextureWithDescriptor:descriptor];
  if (texture == nil) {
    CGContextRelease(context);
    return NO;
  }
  [texture replaceRegion:MTLRegionMake2D(0, 0, (NSUInteger)pixelWidth,
                                         (NSUInteger)pixelHeight)
             mipmapLevel:0
               withBytes:CGBitmapContextGetData(context)
             bytesPerRow:(NSUInteger)pixelWidth * 4];
  CGContextRelease(context);

  surface.selectionLabelTexture = texture;
  surface.selectionLabelText = [text copy];
  surface.selectionLabelScale = scale;
  surface.selectionLabelLightMode = lightMode;
  surface.selectionLabelSize = NSMakeSize(pointWidth, pointHeight);
  return YES;
}

static void redraw_selection_impl(ScreenwidePreviewSurface *surface) {
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
  // Resolved before the vertices are built because the size readout rasterises
  // its own colours from it; both encode paths below reuse this value.
  NSString *appearance = [surface.interaction.effectiveAppearance
      bestMatchFromAppearancesWithNames:@[NSAppearanceNameAqua,
                                          NSAppearanceNameDarkAqua]];
  uint32_t lightMode = [appearance isEqualToString:NSAppearanceNameAqua] ? 1 : 0;
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
  double pixelWidth = 0.0;
  double pixelHeight = 0.0;
  if (selection_pixel_size(surface, &pixelWidth, &pixelHeight)) {
    NSString *text = [NSString stringWithFormat:@"%lld × %lld",
                      (long long)MAX(1, llround(pixelWidth)),
                      (long long)MAX(1, llround(pixelHeight))];
    if (update_selection_label(surface, text, scale, lightMode)) {
      NSSize label = surface.selectionLabelSize;
      // `frame` is already top-left-origin / y-down, so NSMaxY is the box's
      // BOTTOM edge on screen and the readout hangs 4pt below it, trailing
      // edge flush with the box's right edge (Keyframeless's placement).
      CGFloat x = NSMaxX(frame) - label.width;
      CGFloat y = NSMaxY(frame) + 4.0;
      if (y + label.height > size.height)
        y = NSMaxY(frame) - 4.0 - label.height;
      x = MAX(0.0, MIN(x, size.width - label.width));
      y = MAX(0.0, y);
      // Snap to device pixels so the glyphs land on the same grid they were
      // rasterised on and stay crisp instead of resampling every sample.
      x = floor(x * scale) / scale;
      y = floor(y * scale) / scale;
      // The quad's uv (0,0) sits at the rect's min corner, which is the TOP
      // left here; texture row 0 is the bitmap's first memory row, which is
      // the top scanline of the rendered text. So no v flip is needed.
      add_selection_quad(vertices, &count, size,
                         NSMakeRect(x, y, label.width, label.height), 11);
    }
  }
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
    [encoder setFragmentBytes:&lightMode length:sizeof(lightMode) atIndex:0];
    [encoder setFragmentTexture:(surface.selectionLabelTexture
                                     ?: surface.selectionLabelPlaceholder)
                        atIndex:0];
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
  [encoder setFragmentBytes:&lightMode length:sizeof(lightMode) atIndex:0];
  [encoder setFragmentTexture:(surface.selectionLabelTexture
                                   ?: surface.selectionLabelPlaceholder)
                      atIndex:0];
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

SCREENWIDE_PREVIEW_PRIVATE void invalidate_selection_cursor_rects(ScreenwidePreviewSurface *surface);

/// Native GPU overlay extension point for future ruler and annotation OSCs.
@implementation ScreenwidePreviewSurface (OSC)

- (void)redrawSelection {
  redraw_selection_impl(self);
}

@end
