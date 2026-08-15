// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#import <AppKit/AppKit.h>
#import <Metal/Metal.h>
#import <QuartzCore/CAMetalLayer.h>
#import <WebKit/WebKit.h>

#import "cursor_export/gpu_compositor_macos.h"

@interface ScreenwidePreviewView : NSView
@property(nonatomic) BOOL active;
@property(nonatomic) void *compositor;
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

static ScreenwidePreviewView *make_view(ScreenwidePreviewSurface *surface) {
  ScreenwidePreviewView *view = [[ScreenwidePreviewView alloc] initWithFrame:NSZeroRect];
  view.wantsLayer = YES;
  CAMetalLayer *layer = [CAMetalLayer layer];
  layer.device = surface.device;
  layer.pixelFormat = MTLPixelFormatBGRA8Unorm;
  layer.framebufferOnly = NO;
  layer.displaySyncEnabled = YES;
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
  });
  if (surface.pipeline == nil) return NULL;
  return (__bridge_retained void *)surface;
}

void screenwide_preview_surface_set_viewport(void *handle,
                                        double x, double y,
                                        double width, double height,
                                        double red, double green, double blue) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  on_main(^{
    CGFloat host_height = surface.host.bounds.size.height;
    surface.container.frame = NSMakeRect(x, host_height - y - height, width, height);
    // An opaque backstop: while the webview's mask holes and the native pane
    // layout briefly disagree (pan, zoom, resize), the gap shows the app's
    // dark backdrop instead of seeing through the window.
    surface.container.layer.backgroundColor =
        CGColorCreateSRGB(red, green, blue, 1.0);
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
                                  double x, double y, double width, double height) {
  if (handle == NULL) return;
  ScreenwidePreviewSurface *surface = (__bridge ScreenwidePreviewSurface *)handle;
  on_main(^{
    while (surface.views.count <= index) [surface.views addObject:make_view(surface)];
    ScreenwidePreviewView *view = surface.views[index];
    CGFloat viewport_height = surface.container.bounds.size.height;
    view.frame = NSMakeRect(x, viewport_height - y - height, width, height);
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
    for (ScreenwidePreviewView *view in surface.views)
      if (!view.active) view.hidden = YES;
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
  [command presentDrawable:drawable];
  [command commit];
  dispatch_async(dispatch_get_main_queue(), ^{
    if (view.active) {
      surface.container.hidden = NO;
      view.hidden = NO;
    }
  });
  return 1;
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
  int presented = screenwide_gpu_still_presenter_present(
      view.compositor, (__bridge void *)layer, source_token, source_rgba,
      source_width, source_height, canvas, seconds, cursor_rgba, camera_rgba,
      overlay);
  if (presented) dispatch_async(dispatch_get_main_queue(), ^{
    if (view.active) {
      surface.container.hidden = NO;
      view.hidden = NO;
    }
  });
  return presented;
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
  int presented = screenwide_gpu_still_presenter_present_pixels(
      view.compositor, (__bridge void *)layer, source_token, source_pixels,
      canvas, seconds, cursor_rgba, camera_rgba, camera_pixels, overlay);
  if (presented) dispatch_async(dispatch_get_main_queue(), ^{
    if (view.active) {
      surface.container.hidden = NO;
      view.hidden = NO;
    }
  });
  return presented;
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
