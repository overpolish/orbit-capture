// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#import <AppKit/AppKit.h>
#import <Metal/Metal.h>
#import <QuartzCore/CAMetalLayer.h>
#import <QuartzCore/CATransaction.h>
#import <WebKit/WebKit.h>

#import "cursor_export/gpu_compositor_macos.h"

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
@property(nonatomic, strong) NSView *host;
@property(nonatomic, strong) ScreenwidePreviewView *container;
@property(nonatomic, strong) NSMutableArray<ScreenwidePreviewView *> *views;
/// An open present batch: drawables collected between `begin_present` and
/// `end_present` are presented in ONE Core Animation transaction together
/// with every pane's pending frame, so screen, camera and their new rects
/// change on the same display tick.
@property(nonatomic, strong) NSLock *batchLock;
@property(nonatomic) NSInteger batchDepth;
@property(nonatomic, strong) NSMutableArray<id<CAMetalDrawable>> *batchDrawables;
@property(nonatomic, strong) NSMutableArray<ScreenwidePreviewView *> *batchViews;
@end

@implementation ScreenwidePreviewSurface
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
)";

static void on_main(dispatch_block_t block) {
  if ([NSThread isMainThread]) block();
  else dispatch_sync(dispatch_get_main_queue(), block);
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
/// the command buffer to be scheduled before `present`, so that wait happens
/// here on the calling (compositing) thread.
static void present_in_transaction(ScreenwidePreviewSurface *surface,
                                   ScreenwidePreviewView *view,
                                   id<MTLCommandBuffer> command,
                                   id<CAMetalDrawable> drawable) {
  [command commit];
  [command waitUntilScheduled];
  [surface.batchLock lock];
  if (surface.batchDepth > 0) {
    [surface.batchDrawables addObject:drawable];
    [surface.batchViews addObject:view];
    [surface.batchLock unlock];
    return;
  }
  [surface.batchLock unlock];
  run_on_main_transaction(^{
    commit_frames_and_drawables(surface, @[drawable], @[view]);
  });
}

void screenwide_preview_surface_begin_present(void *handle) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  [surface.batchLock lock];
  surface.batchDepth += 1;
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
  [surface.batchDrawables removeAllObjects];
  [surface.batchViews removeAllObjects];
  [surface.batchLock unlock];
  run_on_main_transaction(^{
    commit_frames_and_drawables(surface, drawables, views);
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
    surface.views = [NSMutableArray array];
    surface.batchLock = [NSLock new];
    surface.batchDrawables = [NSMutableArray array];
    surface.batchViews = [NSMutableArray array];
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
  on_main(^{
    CGFloat host_height = surface.host.bounds.size.height;
    surface.container.frame = NSMakeRect(x, host_height - y - height, width, height);
    // An opaque backstop: while the webview's mask holes and the native pane
    // layout briefly disagree (pan, zoom, resize), the gap shows the app's
    // dark backdrop instead of seeing through the window.
    surface.container.layer.backgroundColor =
        CGColorCreateSRGB(red, green, blue, alpha);
    // The webview punches the whole viewport out of its backdrop, so the
    // backstop must be there from the first layout on, not only from the
    // first presented frame. The panes themselves stay hidden until then.
    if (width > 0 && height > 0) surface.container.hidden = NO;
  });
}

void screenwide_preview_surface_begin_layout(void *handle) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  on_main(^{
    for (ScreenwidePreviewView *view in surface.views) view.active = NO;
  });
}

void screenwide_preview_surface_layout(void *handle, uint32_t index,
                                  double x, double y, double width, double height,
                                  int defer_resize) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  on_main(^{
    while (surface.views.count <= index) [surface.views addObject:make_view(surface)];
    ScreenwidePreviewView *view = surface.views[index];
    CGFloat viewport_height = surface.container.bounds.size.height;
    NSRect frame = NSMakeRect(x, viewport_height - y - height, width, height);
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

void screenwide_preview_surface_finish_layout(void *handle) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  on_main(^{
    for (ScreenwidePreviewView *view in surface.views) {
      if (view.active) continue;
      view.hidden = YES;
      view.hasPendingFrame = NO;
    }
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

int screenwide_preview_surface_present_composed(
    void *handle, uint32_t index, uint64_t source_token,
    const uint8_t *source_rgba, uint32_t source_width, uint32_t source_height,
    uint32_t output_width, uint32_t output_height,
    const ScreenwideCanvas *canvas, double seconds, const uint8_t *cursor_rgba,
    const uint8_t *camera_rgba, const ScreenwideStillOverlay *overlay) {
  if (handle == NULL || source_rgba == NULL || canvas == NULL) return 0;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  if (index >= surface.views.count) return 1;
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
  if (index >= surface.views.count) return 1;
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
    surface.container.hidden = YES;
    for (ScreenwidePreviewView *view in surface.views) view.hidden = YES;
  });
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
  });
}
