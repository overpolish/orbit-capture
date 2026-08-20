// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#import <AVFoundation/AVFoundation.h>
#import <Metal/Metal.h>
#import <QuartzCore/CAMetalLayer.h>
#import <VideoToolbox/VideoToolbox.h>
#include <math.h>

#import "gpu_compositor_macos.h"

typedef bool (*ScreenwideShouldCancel)(void *context);
typedef void (*ScreenwideProgress)(void *context, uint64_t position_ms);

/// The artwork description the shader needs, without the caller's bitmap
/// pointer. Slice `style` of the artwork texture array holds the pixels.
typedef struct {
  uint32_t width;
  uint32_t height;
  float design_width;
  float design_height;
  float origin_x;
  float origin_y;
  uint32_t use_design;
  uint32_t clip_local_box;
} ScreenwideCursorArtworkUniforms;

typedef struct {
  int32_t x;
  int32_t y;
  uint32_t cursor_width;
  uint32_t cursor_height;
  uint32_t output_width;
  uint32_t output_height;
  int32_t crop_x;
  int32_t crop_y;
  uint32_t crop_width;
  uint32_t crop_height;
  uint32_t crop_radius;
  uint32_t clip_at_video_edge;
  ScreenwideGpuCursor cursor;
  ScreenwideCursorArtworkUniforms artwork;
} ScreenwideOverlayUniforms;

typedef struct {
  uint32_t crop_x;
  uint32_t crop_y;
  uint32_t crop_width;
  uint32_t crop_height;
  int32_t frame_x;
  int32_t frame_y;
  uint32_t frame_width;
  uint32_t frame_height;
  uint32_t radius;
  uint32_t drop_shadow;
  uint32_t camera_on_top;
} ScreenwideCameraOverlay;

typedef struct {
  uint32_t crop_x;
  uint32_t crop_y;
  uint32_t crop_width;
  uint32_t crop_height;
  int32_t frame_x;
  int32_t frame_y;
  uint32_t frame_width;
  uint32_t frame_height;
  uint32_t radius;
  uint32_t source_width;
  uint32_t source_height;
  uint32_t drop_shadow;
} ScreenwideCameraUniforms;

#import "gpu_compositor_macos_shader_source.h"

static int fail(char *error, size_t capacity, NSString *message) {
  if (error != NULL && capacity > 0) {
    snprintf(error, capacity, "%s",
             (message ?: @"The GPU compositor failed").UTF8String);
  }
  return 0;
}

static NSArray<AVAssetTrack *> *video_tracks(AVURLAsset *asset,
                                             NSError **error) {
  dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
  __block NSArray<AVAssetTrack *> *tracks = nil;
  __block NSError *load_error = nil;
  [asset loadTracksWithMediaType:AVMediaTypeVideo
               completionHandler:^(NSArray<AVAssetTrack *> *loaded,
                                   NSError *failure) {
                 tracks = loaded;
                 load_error = failure;
                 dispatch_semaphore_signal(semaphore);
               }];
  dispatch_semaphore_wait(semaphore, DISPATCH_TIME_FOREVER);
  if (error != NULL)
    *error = load_error;
  return tracks;
}

/// Uploads one RGBA slice per cursor style. Every slice shares the largest
/// artwork's dimensions; each style only ever samples its own recorded size.
static id<MTLTexture> cursor_artwork_texture(
    id<MTLDevice> device, const ScreenwideCursorArtwork *artworks,
    uint32_t count) {
  if (artworks == NULL || count == 0)
    return nil;
  uint32_t width = 0;
  uint32_t height = 0;
  for (uint32_t index = 0; index < count; ++index) {
    if (artworks[index].pixels == NULL)
      continue;
    width = MAX(width, artworks[index].width);
    height = MAX(height, artworks[index].height);
  }
  if (width == 0 || height == 0)
    return nil;
  MTLTextureDescriptor *description = [[MTLTextureDescriptor alloc] init];
  description.textureType = MTLTextureType2DArray;
  description.pixelFormat = MTLPixelFormatRGBA8Unorm;
  description.width = width;
  description.height = height;
  description.arrayLength = count;
  description.usage = MTLTextureUsageShaderRead;
  id<MTLTexture> texture = [device newTextureWithDescriptor:description];
  if (texture == nil)
    return nil;
  for (uint32_t index = 0; index < count; ++index) {
    const ScreenwideCursorArtwork *artwork = &artworks[index];
    if (artwork->pixels == NULL || artwork->width == 0 || artwork->height == 0)
      continue;
    [texture replaceRegion:MTLRegionMake2D(0, 0, artwork->width, artwork->height)
               mipmapLevel:0
                     slice:index
                 withBytes:artwork->pixels
               bytesPerRow:(NSUInteger)artwork->width * 4
             bytesPerImage:(NSUInteger)artwork->width * artwork->height * 4];
  }
  return texture;
}

/// The cursor for one output frame. Frame `n` of the 60 Hz cursor grid becomes
/// current the moment its own timestamp is reached, which is exactly how the
/// retired positions sidecar was indexed against the cursor movie.
static const ScreenwideGpuCursor *cursor_at(const ScreenwideGpuCursor *cursors,
                                            uint32_t count, CMTime pts) {
  if (cursors == NULL || count == 0)
    return NULL;
  double seconds = CMTimeGetSeconds(pts);
  if (!isfinite(seconds) || seconds < 0.0)
    seconds = 0.0;
  // Frame timestamps are exact sixtieths, so a presentation time landing on
  // one must select it rather than its predecessor.
  int64_t index = (int64_t)floor(seconds * 60.0 + 1e-6);
  if (index > (int64_t)count - 1)
    index = (int64_t)count - 1;
  return &cursors[index];
}

/// Draws the cursor into the composed frame's planes. The shader owns the
/// pixels; this only sizes the dispatch to the cursor's bounds
/// (`raster::bounds`, cursor_effects/raster.rs:294-307).
static void encode_cursor_overlay(
    id<MTLCommandBuffer> command, id<MTLComputePipelineState> luma_pipeline,
    id<MTLComputePipelineState> chroma_pipeline, id<MTLTexture> destination_y,
    id<MTLTexture> destination_uv, id<MTLTexture> artwork_texture,
    const ScreenwideGpuCursor *cursor, const ScreenwideCursorArtwork *artworks,
    uint32_t artwork_count, const ScreenwideCanvas *canvas,
    uint32_t output_width, uint32_t output_height) {
  if (cursor == NULL || cursor->visible == 0 || artwork_texture == nil ||
      cursor->style >= artwork_count)
    return;
  const ScreenwideCursorArtwork *artwork = &artworks[cursor->style];
  if (artwork->pixels == NULL || artwork->width == 0 || artwork->height == 0)
    return;
  double travel = hypot(cursor->blur_delta_x, cursor->blur_delta_y);
  double distance = MIN(travel, 80.0);
  double blur = distance > 1.25 ? distance : 0.0;
  double radius = hypot(cursor->width, cursor->height) * cursor->scale;
  double left = floor(cursor->x - radius - blur);
  double top = floor(cursor->y - radius - blur);
  double right = ceil(cursor->x + radius + blur);
  double bottom = ceil(cursor->y + radius + blur);
  left = MAX(left, 0.0);
  top = MAX(top, 0.0);
  right = MIN(right, (double)output_width);
  bottom = MIN(bottom, (double)output_height);
  // A chroma thread owns the four output pixels of one chroma sample, so an
  // even origin keeps two threads from writing the same chroma pixel.
  int32_t x = (int32_t)left & ~1;
  int32_t y = (int32_t)top & ~1;
  if (right <= (double)x || bottom <= (double)y)
    return;
  uint32_t box_width = (uint32_t)(right - (double)x);
  uint32_t box_height = (uint32_t)(bottom - (double)y);
  ScreenwideOverlayUniforms uniforms = {
      x,
      y,
      box_width,
      box_height,
      output_width,
      output_height,
      canvas->crop_x,
      canvas->crop_y,
      canvas->crop_width,
      canvas->crop_height,
      canvas->radius,
      // The frame carries the same setting the canvas does; taking it from the
      // cursor keeps one owner of the effect for the drawn cursor.
      cursor->clip_at_video_edge,
      *cursor,
      {
          artwork->width,
          artwork->height,
          artwork->design_width,
          artwork->design_height,
          artwork->origin_x,
          artwork->origin_y,
          artwork->use_design,
          artwork->clip_local_box,
      },
  };
  MTLSize group = MTLSizeMake(16, 16, 1);
  id<MTLComputeCommandEncoder> compute = [command computeCommandEncoder];
  [compute setComputePipelineState:luma_pipeline];
  [compute setTexture:artwork_texture atIndex:0];
  [compute setTexture:destination_y atIndex:1];
  [compute setBytes:&uniforms length:sizeof(uniforms) atIndex:0];
  [compute dispatchThreads:MTLSizeMake(box_width, box_height, 1)
      threadsPerThreadgroup:group];
  [compute endEncoding];
  compute = [command computeCommandEncoder];
  [compute setComputePipelineState:chroma_pipeline];
  [compute setTexture:artwork_texture atIndex:0];
  [compute setTexture:destination_uv atIndex:1];
  [compute setBytes:&uniforms length:sizeof(uniforms) atIndex:0];
  [compute dispatchThreads:MTLSizeMake((box_width + 1) / 2,
                                       (box_height + 1) / 2, 1)
      threadsPerThreadgroup:group];
  [compute endEncoding];
}

static AVAssetReaderTrackOutput *
reader_output(AVAssetReader *reader, AVAssetTrack *track, OSType format,
              NSNumber *width, NSNumber *height, NSError **error) {
  NSMutableDictionary *settings = [@{
    (NSString *)kCVPixelBufferPixelFormatTypeKey : @(format),
    (NSString *)kCVPixelBufferMetalCompatibilityKey : @YES,
    (NSString *)kCVPixelBufferIOSurfacePropertiesKey : @{},
  } mutableCopy];
  if (width != nil)
    settings[(NSString *)kCVPixelBufferWidthKey] = width;
  if (height != nil)
    settings[(NSString *)kCVPixelBufferHeightKey] = height;
  AVAssetReaderTrackOutput *output =
      [[AVAssetReaderTrackOutput alloc] initWithTrack:track
                                       outputSettings:settings];
  output.alwaysCopiesSampleData = NO;
  if (![reader canAddOutput:output]) {
    if (error != NULL) {
      *error = [NSError errorWithDomain:@"ScreenwideGPUCompositor"
                                   code:1
                               userInfo:@{
                                 NSLocalizedDescriptionKey :
                                     @"AVFoundation rejected a GPU video reader"
                               }];
    }
    return nil;
  }
  [reader addOutput:output];
  return output;
}

static id<MTLTexture> texture(CVMetalTextureCacheRef cache,
                              CVPixelBufferRef pixels, MTLPixelFormat format,
                              size_t width, size_t height, size_t plane,
                              CVMetalTextureRef *reference) {
  CVReturn result = CVMetalTextureCacheCreateTextureFromImage(
      kCFAllocatorDefault, cache, pixels, NULL, format, width, height, plane,
      reference);
  if (result != kCVReturnSuccess || *reference == NULL)
    return nil;
  return CVMetalTextureGetTexture(*reference);
}

/// How many composited frames may sit on the GPU while the loop keeps
/// decoding. A four second sample of the old fully serial loop (decode ->
/// composite -> block on the GPU -> poll the encoder -> append) spent 63% of
/// its time sleeping on `readyForMoreMediaData` and 34% inside
/// `waitUntilCompleted`, which put a 3494x2260 export at 0.4x realtime: every
/// stage idled while another worked. Keeping three frames in flight lets the
/// encoder chew on frame K while the GPU composites K+1 and the reader decodes
/// K+2, so the two waits overlap with real work instead of each other. Three is
/// enough to cover both stalls (the deepest measured stall was one frame of
/// encoder backpressure) without holding many 4K frame buffers - the ring costs
/// this many pool buffers plus the one being encoded, and the reader's own
/// sample buffers stay retained just as long.
#define SCREENWIDE_GPU_INFLIGHT_FRAMES 3

/// One frame whose Metal work is committed but not yet handed to the writer.
/// Everything the command buffer touches lives here until it completes: the
/// output pixel buffer, the Metal textures wrapping it and the source, and the
/// sample buffers those source textures read from.
@interface ScreenwideInflightFrame : NSObject
@property(nonatomic, strong) id<MTLCommandBuffer> command;
@property(nonatomic) CMTime presentation;
@property(nonatomic) CVPixelBufferRef destination;
@property(nonatomic) CVMetalTextureRef sourceLuma;
@property(nonatomic) CVMetalTextureRef sourceChroma;
@property(nonatomic) CVMetalTextureRef destinationLuma;
@property(nonatomic) CVMetalTextureRef destinationChroma;
@property(nonatomic) CVMetalTextureRef cameraTexture;
@property(nonatomic) CMSampleBufferRef screenSample;
@property(nonatomic) CMSampleBufferRef cameraSample;
@end

@implementation ScreenwideInflightFrame
- (void)dealloc {
  if (_destination != NULL)
    CVPixelBufferRelease(_destination);
  if (_sourceLuma != NULL)
    CFRelease(_sourceLuma);
  if (_sourceChroma != NULL)
    CFRelease(_sourceChroma);
  if (_destinationLuma != NULL)
    CFRelease(_destinationLuma);
  if (_destinationChroma != NULL)
    CFRelease(_destinationChroma);
  if (_cameraTexture != NULL)
    CFRelease(_cameraTexture);
  if (_screenSample != NULL)
    CFRelease(_screenSample);
  if (_cameraSample != NULL)
    CFRelease(_cameraSample);
}
@end

typedef enum {
  ScreenwideDrainAppended,
  ScreenwideDrainCancelled,
  ScreenwideDrainFailed,
} ScreenwideDrainResult;

/// Waits for the oldest in-flight frame and appends it. The ring pops
/// oldest-first, so source order is structural; by the time this waits the
/// command buffer is almost always already done and the encoder has had three
/// frames worth of head start.
static ScreenwideDrainResult
drain_inflight_frame(ScreenwideInflightFrame *frame,
                     AVAssetWriterInputPixelBufferAdaptor *adaptor,
                     AVAssetWriterInput *input, AVAssetWriter *writer,
                     void *context, ScreenwideShouldCancel should_cancel,
                     ScreenwideProgress progress, BOOL *primed,
                     float source_frame_rate, NSError **error) {
  [frame.command waitUntilCompleted];
  if (frame.command.status == MTLCommandBufferStatusError) {
    *error = frame.command.error
                 ?: [NSError errorWithDomain:@"ScreenwideGPUCompositor"
                                        code:3
                                    userInfo:@{
                                      NSLocalizedDescriptionKey :
                                          @"The GPU encoder rejected a video frame"
                                    }];
    return ScreenwideDrainFailed;
  }
  // The residual encoder wait. It now overlaps with the newer frames whose
  // decode and GPU work is already in flight, which is the point of the ring.
  while (!input.isReadyForMoreMediaData) {
    if (should_cancel != NULL && should_cancel(context))
      return ScreenwideDrainCancelled;
    [NSThread sleepForTimeInterval:0.001];
  }
  if (!*primed) {
    *primed = YES;
    // Hardware rate control ramps up over its first seconds (the first
    // keyframe of an export measures at half the steady-state size).
    // Feeding the first frame repeatedly at negative timestamps warms
    // the encoder on samples the edit list trims away, so the visible
    // first frame starts at steady-state quality.
    int32_t warm_fps = (int32_t)MAX(llround(source_frame_rate), 1);
    for (int32_t warm = 45; warm >= 1; warm--) {
      while (!input.isReadyForMoreMediaData)
        [NSThread sleepForTimeInterval:0.001];
      if (![adaptor appendPixelBuffer:frame.destination
                 withPresentationTime:CMTimeMake(-warm, warm_fps)])
        break;
    }
    while (!input.isReadyForMoreMediaData)
      [NSThread sleepForTimeInterval:0.001];
  }
  if (![adaptor appendPixelBuffer:frame.destination
             withPresentationTime:frame.presentation]) {
    *error = writer.error
                 ?: [NSError errorWithDomain:@"ScreenwideGPUCompositor"
                                        code:3
                                    userInfo:@{
                                      NSLocalizedDescriptionKey :
                                          @"The GPU encoder rejected a video frame"
                                    }];
    return ScreenwideDrainFailed;
  }
  if (progress != NULL)
    progress(context,
             (uint64_t)llround(CMTimeGetSeconds(frame.presentation) * 1000.0));
  return ScreenwideDrainAppended;
}

int screenwide_gpu_composite_cursor(const char *screen_path,
                               const ScreenwideGpuCursor *cursors,
                               uint32_t cursor_count,
                               const ScreenwideCursorArtwork *artworks,
                               uint32_t artwork_count,
                               const char *camera_path,
                               const ScreenwideCameraOverlay *camera_overlay,
                               const ScreenwideCanvas *canvas,
                               const char *output_path, uint32_t source_width,
                               uint32_t source_height, uint32_t output_width,
                               uint32_t output_height, uint64_t bitrate,
                               void *context, ScreenwideShouldCancel should_cancel,
                               ScreenwideProgress progress, char *error_text,
                               size_t error_capacity) {
  (void)source_width;
  (void)source_height;
  @autoreleasepool {
    NSError *error = nil;
    AVURLAsset *screen_asset =
        [AVURLAsset URLAssetWithURL:[NSURL fileURLWithPath:@(screen_path)]
                            options:nil];
    AVURLAsset *camera_asset = camera_path == NULL
                                   ? nil
                                   : [AVURLAsset
                                         URLAssetWithURL:[NSURL fileURLWithPath:
                                                                      @(camera_path)]
                                                  options:nil];
    AVAssetTrack *screen_track = video_tracks(screen_asset, &error).firstObject;
    if (screen_track == nil && error != nil)
      return fail(error_text, error_capacity, error.localizedDescription);
    AVAssetTrack *camera_track =
        camera_asset == nil ? nil : video_tracks(camera_asset, &error).firstObject;
    if (screen_track == nil)
      return fail(error_text, error_capacity,
                  @"The GPU compositor could not find the recording track");
    if (camera_asset != nil && camera_track == nil)
      return fail(error_text, error_capacity,
                  error.localizedDescription ?:
                    @"The GPU compositor could not find the camera track");

    AVAssetReader *screen_reader =
        [[AVAssetReader alloc] initWithAsset:screen_asset error:&error];
    AVAssetReaderTrackOutput *screen_output =
        reader_output(screen_reader, screen_track,
                      kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
                      nil, nil, &error);
    if (screen_output == nil)
      return fail(error_text, error_capacity, error.localizedDescription);
    AVAssetReader *camera_reader =
        camera_asset == nil
            ? nil
            : [[AVAssetReader alloc] initWithAsset:camera_asset error:&error];
    AVAssetReaderTrackOutput *camera_output =
        camera_reader == nil
            ? nil
            : reader_output(camera_reader, camera_track,
                            kCVPixelFormatType_32BGRA, nil, nil, &error);
    if (camera_reader != nil && camera_output == nil)
      return fail(error_text, error_capacity, error.localizedDescription);

    NSURL *output_url = [NSURL fileURLWithPath:@(output_path)];
    [[NSFileManager defaultManager] removeItemAtURL:output_url error:nil];
    AVAssetWriter *writer = [[AVAssetWriter alloc] initWithURL:output_url
                                                      fileType:AVFileTypeMPEG4
                                                         error:&error];
    if (writer == nil)
      return fail(error_text, error_capacity, error.localizedDescription);
    writer.shouldOptimizeForNetworkUse = YES;
    float source_frame_rate = screen_track.nominalFrameRate;
    if (!isfinite(source_frame_rate) || source_frame_rate < 1.0)
      source_frame_rate = 60.0;
    NSNumber *expected_frame_rate = @((NSInteger)llround(source_frame_rate));
    NSDictionary *video_settings = @{
      AVVideoCodecKey : AVVideoCodecTypeH264,
      AVVideoWidthKey : @(output_width),
      AVVideoHeightKey : @(output_height),
      AVVideoCompressionPropertiesKey : @{
        AVVideoAverageBitRateKey : @(bitrate),
        AVVideoExpectedSourceFrameRateKey : expected_frame_rate,
        AVVideoAverageNonDroppableFrameRateKey : expected_frame_rate,
        AVVideoMaxKeyFrameIntervalKey : @(expected_frame_rate.integerValue * 4),
        AVVideoMaxKeyFrameIntervalDurationKey : @4,
        AVVideoAllowFrameReorderingKey : @NO,
        AVVideoH264EntropyModeKey : AVVideoH264EntropyModeCABAC,
        AVVideoProfileLevelKey : AVVideoProfileLevelH264HighAutoLevel,
      },
    };
    AVAssetWriterInput *writer_input =
        [[AVAssetWriterInput alloc] initWithMediaType:AVMediaTypeVideo
                                       outputSettings:video_settings];
    NSDictionary *pixel_attributes = @{
      (NSString *)kCVPixelBufferPixelFormatTypeKey :
          @(kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange),
      (NSString *)kCVPixelBufferWidthKey : @(output_width),
      (NSString *)kCVPixelBufferHeightKey : @(output_height),
      (NSString *)kCVPixelBufferMetalCompatibilityKey : @YES,
      (NSString *)kCVPixelBufferIOSurfacePropertiesKey : @{},
    };
    AVAssetWriterInputPixelBufferAdaptor *adaptor =
        [[AVAssetWriterInputPixelBufferAdaptor alloc]
               initWithAssetWriterInput:writer_input
            sourcePixelBufferAttributes:pixel_attributes];
    if (![writer canAddInput:writer_input])
      return fail(error_text, error_capacity,
                  @"AVFoundation rejected the GPU video writer");
    [writer addInput:writer_input];

    id<MTLDevice> device = MTLCreateSystemDefaultDevice();
    id<MTLLibrary> library = [device newLibraryWithSource:shader_source
                                                  options:nil
                                                    error:&error];
    id<MTLComputePipelineState> luma_pipeline =
        [device newComputePipelineStateWithFunction:
                    [library newFunctionWithName:@"overlay_luma"]
                                              error:&error];
    id<MTLComputePipelineState> chroma_pipeline =
        [device newComputePipelineStateWithFunction:
                    [library newFunctionWithName:@"overlay_chroma"]
                                              error:&error];
    id<MTLComputePipelineState> camera_luma_pipeline =
        [device newComputePipelineStateWithFunction:
                    [library newFunctionWithName:@"overlay_camera_luma"]
                                              error:&error];
    id<MTLComputePipelineState> camera_chroma_pipeline =
        [device newComputePipelineStateWithFunction:
                    [library newFunctionWithName:@"overlay_camera_chroma"]
                                              error:&error];
    id<MTLComputePipelineState> canvas_luma_pipeline =
        [device newComputePipelineStateWithFunction:
                    [library newFunctionWithName:@"compose_canvas_luma"]
                                              error:&error];
    id<MTLComputePipelineState> canvas_chroma_pipeline =
        [device newComputePipelineStateWithFunction:
                    [library newFunctionWithName:@"compose_canvas_chroma"]
                                              error:&error];
    id<MTLComputePipelineState> screen_luma_pipeline =
        [device newComputePipelineStateWithFunction:
                    [library newFunctionWithName:@"overlay_screen_luma"]
                                              error:&error];
    id<MTLComputePipelineState> screen_chroma_pipeline =
        [device newComputePipelineStateWithFunction:
                    [library newFunctionWithName:@"overlay_screen_chroma"]
                                              error:&error];
    id<MTLCommandQueue> queue = [device newCommandQueue];
    id<MTLTexture> cursor_artwork =
        cursor_artwork_texture(device, artworks, artwork_count);
    CVMetalTextureCacheRef texture_cache = NULL;
    CVMetalTextureCacheCreate(kCFAllocatorDefault, NULL, device, NULL,
                              &texture_cache);
    if (device == nil || library == nil || luma_pipeline == nil ||
        chroma_pipeline == nil || camera_luma_pipeline == nil ||
        camera_chroma_pipeline == nil || queue == nil ||
        canvas_luma_pipeline == nil || canvas_chroma_pipeline == nil ||
        screen_luma_pipeline == nil || screen_chroma_pipeline == nil ||
        texture_cache == NULL)
      return fail(error_text, error_capacity,
                  error.localizedDescription
                      ?: @"The Metal cursor shader could not be created");

    if (![screen_reader startReading] ||
        (camera_reader != nil && ![camera_reader startReading]) ||
        ![writer startWriting]) {
      CFRelease(texture_cache);
      return fail(error_text, error_capacity,
                  screen_reader.error.localizedDescription ?:
                    (camera_reader != nil ? camera_reader.error.localizedDescription : nil) ?:
                    writer.error.localizedDescription ?:
                    @"The GPU export could not be started");
    }
    [writer startSessionAtSourceTime:kCMTimeZero];
    BOOL primed = NO;
    // Frames whose Metal work is committed but not yet appended, oldest first.
    NSMutableArray<ScreenwideInflightFrame *> *ring =
        [NSMutableArray arrayWithCapacity:SCREENWIDE_GPU_INFLIGHT_FRAMES + 1];
    CMSampleBufferRef camera_sample = NULL;
    CMSampleBufferRef next_camera_sample = camera_output == nil
        ? NULL : [camera_output copyNextSampleBuffer];
    bool cancelled = false;
    CMSampleBufferRef screen_sample = NULL;
    while ((screen_sample = [screen_output copyNextSampleBuffer]) != NULL) {
      @autoreleasepool {
        if (should_cancel != NULL && should_cancel(context)) {
          cancelled = true;
          CFRelease(screen_sample);
          break;
        }
        CMTime pts = CMSampleBufferGetPresentationTimeStamp(screen_sample);
        const ScreenwideGpuCursor *cursor = cursor_at(cursors, cursor_count, pts);
        while (next_camera_sample != NULL &&
               CMTimeCompare(
                   CMSampleBufferGetPresentationTimeStamp(next_camera_sample),
                   pts) <= 0) {
          if (camera_sample != NULL)
            CFRelease(camera_sample);
          camera_sample = next_camera_sample;
          next_camera_sample = [camera_output copyNextSampleBuffer];
        }
        CVPixelBufferRef destination = NULL;
        if (CVPixelBufferPoolCreatePixelBuffer(
                kCFAllocatorDefault, adaptor.pixelBufferPool, &destination) !=
                kCVReturnSuccess ||
            destination == NULL) {
          CFRelease(screen_sample);
          error =
              [NSError errorWithDomain:@"ScreenwideGPUCompositor"
                                  code:2
                              userInfo:@{
                                NSLocalizedDescriptionKey :
                                    @"The GPU encoder ran out of video buffers"
                              }];
          break;
        }
        CVPixelBufferRef source = CMSampleBufferGetImageBuffer(screen_sample);
        size_t source_y_width = CVPixelBufferGetWidthOfPlane(source, 0);
        size_t source_y_height = CVPixelBufferGetHeightOfPlane(source, 0);
        size_t source_uv_width = CVPixelBufferGetWidthOfPlane(source, 1);
        size_t source_uv_height = CVPixelBufferGetHeightOfPlane(source, 1);
        size_t y_width = output_width;
        size_t y_height = output_height;
        size_t uv_width = (output_width + 1) / 2;
        size_t uv_height = (output_height + 1) / 2;
        CVMetalTextureRef source_y_ref = NULL, source_uv_ref = NULL;
        CVMetalTextureRef destination_y_ref = NULL, destination_uv_ref = NULL;
        id<MTLTexture> source_y =
            texture(texture_cache, source, MTLPixelFormatR8Unorm, source_y_width,
                    source_y_height, 0, &source_y_ref);
        id<MTLTexture> source_uv =
            texture(texture_cache, source, MTLPixelFormatRG8Unorm, source_uv_width,
                    source_uv_height, 1, &source_uv_ref);
        id<MTLTexture> destination_y =
            texture(texture_cache, destination, MTLPixelFormatR8Unorm, y_width,
                    y_height, 0, &destination_y_ref);
        id<MTLTexture> destination_uv =
            texture(texture_cache, destination, MTLPixelFormatRG8Unorm,
                    uv_width, uv_height, 1, &destination_uv_ref);
        id<MTLCommandBuffer> command = [queue commandBuffer];
        float seconds = (float)CMTimeGetSeconds(pts);
        MTLSize canvas_group = MTLSizeMake(16, 16, 1);
        id<MTLComputeCommandEncoder> canvas_compute =
            [command computeCommandEncoder];
        [canvas_compute setComputePipelineState:canvas_luma_pipeline];
        [canvas_compute setTexture:source_y atIndex:0];
        [canvas_compute setTexture:source_uv atIndex:1];
        [canvas_compute setTexture:destination_y atIndex:2];
        [canvas_compute setBytes:canvas length:sizeof(*canvas) atIndex:0];
        [canvas_compute setBytes:&seconds length:sizeof(seconds) atIndex:1];
        [canvas_compute dispatchThreads:MTLSizeMake(y_width, y_height, 1)
                     threadsPerThreadgroup:canvas_group];
        [canvas_compute endEncoding];
        canvas_compute = [command computeCommandEncoder];
        [canvas_compute setComputePipelineState:canvas_chroma_pipeline];
        [canvas_compute setTexture:source_y atIndex:0];
        [canvas_compute setTexture:source_uv atIndex:1];
        [canvas_compute setTexture:destination_uv atIndex:2];
        [canvas_compute setBytes:canvas length:sizeof(*canvas) atIndex:0];
        [canvas_compute setBytes:&seconds length:sizeof(seconds) atIndex:1];
        [canvas_compute dispatchThreads:MTLSizeMake(uv_width, uv_height, 1)
                     threadsPerThreadgroup:canvas_group];
        [canvas_compute endEncoding];
        encode_cursor_overlay(command, luma_pipeline, chroma_pipeline,
                              destination_y, destination_uv, cursor_artwork,
                              cursor, artworks, artwork_count, canvas,
                              output_width, output_height);
        CVMetalTextureRef camera_ref = NULL;
        if (camera_sample != NULL && camera_overlay != NULL) {
          CVPixelBufferRef camera_pixels =
              CMSampleBufferGetImageBuffer(camera_sample);
          size_t camera_width = CVPixelBufferGetWidth(camera_pixels);
          size_t camera_height = CVPixelBufferGetHeight(camera_pixels);
          id<MTLTexture> camera_texture =
              texture(texture_cache, camera_pixels, MTLPixelFormatBGRA8Unorm,
                      camera_width, camera_height, 0, &camera_ref);
          ScreenwideCameraUniforms camera_uniforms = {
              camera_overlay->crop_x,
              camera_overlay->crop_y,
              camera_overlay->crop_width,
              camera_overlay->crop_height,
              camera_overlay->frame_x,
              camera_overlay->frame_y,
              camera_overlay->frame_width,
              camera_overlay->frame_height,
              camera_overlay->radius,
              (uint32_t)camera_width,
              (uint32_t)camera_height,
              camera_overlay->drop_shadow,
          };
          MTLSize camera_group = MTLSizeMake(16, 16, 1);
          id<MTLComputeCommandEncoder> camera_compute =
              [command computeCommandEncoder];
          [camera_compute setComputePipelineState:camera_luma_pipeline];
          [camera_compute setTexture:camera_texture atIndex:0];
          [camera_compute setTexture:destination_y atIndex:1];
          [camera_compute setBytes:&camera_uniforms
                            length:sizeof(camera_uniforms)
                           atIndex:0];
          [camera_compute
              dispatchThreads:MTLSizeMake(y_width, y_height, 1)
              threadsPerThreadgroup:camera_group];
          [camera_compute endEncoding];
          camera_compute = [command computeCommandEncoder];
          [camera_compute setComputePipelineState:camera_chroma_pipeline];
          [camera_compute setTexture:camera_texture atIndex:0];
          [camera_compute setTexture:destination_uv atIndex:1];
          [camera_compute setBytes:&camera_uniforms
                            length:sizeof(camera_uniforms)
                           atIndex:0];
          [camera_compute
              dispatchThreads:MTLSizeMake(uv_width, uv_height, 1)
              threadsPerThreadgroup:camera_group];
          [camera_compute endEncoding];
        }
        if (camera_sample != NULL && camera_overlay != NULL &&
            camera_overlay->camera_on_top == 0) {
          MTLSize screen_group = MTLSizeMake(16, 16, 1);
          id<MTLComputeCommandEncoder> screen_compute =
              [command computeCommandEncoder];
          [screen_compute setComputePipelineState:screen_luma_pipeline];
          [screen_compute setTexture:source_y atIndex:0];
          [screen_compute setTexture:source_uv atIndex:1];
          [screen_compute setTexture:destination_y atIndex:2];
          [screen_compute setBytes:canvas length:sizeof(*canvas) atIndex:0];
          [screen_compute dispatchThreads:MTLSizeMake(y_width, y_height, 1)
                       threadsPerThreadgroup:screen_group];
          [screen_compute endEncoding];
          screen_compute = [command computeCommandEncoder];
          [screen_compute setComputePipelineState:screen_chroma_pipeline];
          [screen_compute setTexture:source_y atIndex:0];
          [screen_compute setTexture:source_uv atIndex:1];
          [screen_compute setTexture:destination_uv atIndex:2];
          [screen_compute setBytes:canvas length:sizeof(*canvas) atIndex:0];
          [screen_compute dispatchThreads:MTLSizeMake(uv_width, uv_height, 1)
                       threadsPerThreadgroup:screen_group];
          [screen_compute endEncoding];

          // Cursor belongs to the screen layer. Reapply it after the screen
          // when the camera has been sent behind that layer.
          encode_cursor_overlay(command, luma_pipeline, chroma_pipeline,
                                destination_y, destination_uv, cursor_artwork,
                                cursor, artworks, artwork_count, canvas,
                                output_width, output_height);
        }
        // Commit without waiting: the GPU works on this frame while the loop
        // decodes and composites the next ones. The frame owns every resource
        // the command buffer reads or writes until it completes.
        [command commit];
        ScreenwideInflightFrame *frame = [ScreenwideInflightFrame new];
        frame.command = command;
        frame.presentation = pts;
        frame.destination = destination;
        frame.sourceLuma = source_y_ref;
        frame.sourceChroma = source_uv_ref;
        frame.destinationLuma = destination_y_ref;
        frame.destinationChroma = destination_uv_ref;
        frame.cameraTexture = camera_ref;
        frame.screenSample = screen_sample;
        frame.cameraSample =
            camera_sample == NULL ? NULL : (CMSampleBufferRef)CFRetain(camera_sample);
        [ring addObject:frame];
        if (ring.count > SCREENWIDE_GPU_INFLIGHT_FRAMES) {
          ScreenwideInflightFrame *oldest = ring.firstObject;
          [ring removeObjectAtIndex:0];
          ScreenwideDrainResult drained = drain_inflight_frame(
              oldest, adaptor, writer_input, writer, context, should_cancel,
              progress, &primed, source_frame_rate, &error);
          if (drained == ScreenwideDrainCancelled)
            cancelled = true;
          if (drained != ScreenwideDrainAppended)
            break;
        }
      }
    }
    // Flush whatever is still on the GPU, oldest first, so the tail of the
    // export keeps its source order.
    while (!cancelled && error == nil && ring.count > 0) {
      @autoreleasepool {
        ScreenwideInflightFrame *oldest = ring.firstObject;
        [ring removeObjectAtIndex:0];
        ScreenwideDrainResult drained = drain_inflight_frame(
            oldest, adaptor, writer_input, writer, context, should_cancel,
            progress, &primed, source_frame_rate, &error);
        if (drained == ScreenwideDrainCancelled)
          cancelled = true;
        if (drained != ScreenwideDrainAppended)
          break;
      }
    }
    // A cancel or a writer failure abandons the rest of the ring. The GPU still
    // owns those textures, so wait for each command buffer before the frames
    // release their buffers.
    for (ScreenwideInflightFrame *abandoned in ring)
      [abandoned.command waitUntilCompleted];
    [ring removeAllObjects];
    if (camera_sample != NULL)
      CFRelease(camera_sample);
    if (next_camera_sample != NULL)
      CFRelease(next_camera_sample);
    CFRelease(texture_cache);
    if (cancelled) {
      [screen_reader cancelReading];
      if (camera_reader != nil) [camera_reader cancelReading];
      [writer cancelWriting];
      [[NSFileManager defaultManager] removeItemAtURL:output_url error:nil];
      return -1;
    }
    if (error != nil || screen_reader.status == AVAssetReaderStatusFailed ||
        (camera_reader != nil && camera_reader.status == AVAssetReaderStatusFailed)) {
      [writer cancelWriting];
      [[NSFileManager defaultManager] removeItemAtURL:output_url error:nil];
      return fail(error_text, error_capacity,
                  error.localizedDescription ?:
                    screen_reader.error.localizedDescription ?:
                    (camera_reader != nil ? camera_reader.error.localizedDescription : nil) ?:
                    @"The GPU compositor could not read the recording");
    }
    [writer_input markAsFinished];
    dispatch_semaphore_t finish_semaphore = dispatch_semaphore_create(0);
    [writer finishWritingWithCompletionHandler:^{
      dispatch_semaphore_signal(finish_semaphore);
    }];
    dispatch_semaphore_wait(finish_semaphore, DISPATCH_TIME_FOREVER);
    if (writer.status != AVAssetWriterStatusCompleted) {
      [[NSFileManager defaultManager] removeItemAtURL:output_url error:nil];
      return fail(error_text, error_capacity,
                  writer.error.localizedDescription
                      ?: @"The GPU encoder could not finish the recording");
    }
    return 1;
  }
}

int screenwide_gpu_composite_still(const uint8_t *source_rgba,
                              uint32_t source_width,
                              uint32_t source_height,
                              const ScreenwideCanvas *canvas,
                              uint32_t output_width,
                              uint32_t output_height,
                              double seconds,
                              const uint8_t *cursor_rgba,
                              const uint8_t *camera_rgba,
                              const ScreenwideStillOverlay *overlay,
                              uint8_t *output_rgba,
                              char *error_text,
                              size_t error_capacity) {
  @autoreleasepool {
    if (source_rgba == NULL || output_rgba == NULL || canvas == NULL ||
        source_width == 0 || source_height == 0 ||
        output_width == 0 || output_height == 0) {
      return fail(error_text, error_capacity,
                  @"The GPU still compositor received invalid pixels");
    }
    static id<MTLDevice> device;
    static id<MTLComputePipelineState> pipeline;
    static id<MTLCommandQueue> queue;
    static NSString *initialization_error;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
      NSError *error = nil;
      device = MTLCreateSystemDefaultDevice();
      id<MTLLibrary> library = [device newLibraryWithSource:shader_source
                                                    options:nil
                                                      error:&error];
      id<MTLFunction> function =
          [library newFunctionWithName:@"compose_canvas_rgba"];
      pipeline =
          [device newComputePipelineStateWithFunction:function error:&error];
      queue = [device newCommandQueue];
      initialization_error = error.localizedDescription;
    });
    if (device == nil || pipeline == nil || queue == nil) {
      return fail(error_text, error_capacity,
                  initialization_error ?:
                    @"The Metal still compositor could not be created");
    }
    NSUInteger source_length =
        (NSUInteger)source_width * source_height * 4;
    NSUInteger output_length =
        (NSUInteger)output_width * output_height * 4;
    id<MTLBuffer> source = [device newBufferWithBytes:source_rgba
                                               length:source_length
                                              options:MTLResourceStorageModeShared];
    id<MTLBuffer> output = [device newBufferWithLength:output_length
                                               options:MTLResourceStorageModeShared];
    id<MTLBuffer> uniforms = [device newBufferWithBytes:canvas
                                                 length:sizeof(*canvas)
                                                options:MTLResourceStorageModeShared];
    ScreenwideStillOverlay empty_overlay = {0};
    if (overlay == NULL) overlay = &empty_overlay;
    id<MTLBuffer> cursor = cursor_rgba == NULL
        ? [device newBufferWithLength:4 options:MTLResourceStorageModeShared]
        : [device newBufferWithBytes:cursor_rgba
                              length:(NSUInteger)overlay->cursor_source_width *
                                     overlay->cursor_source_height * 4
                             options:MTLResourceStorageModeShared];
    id<MTLBuffer> camera = camera_rgba == NULL
        ? [device newBufferWithLength:4 options:MTLResourceStorageModeShared]
        : [device newBufferWithBytes:camera_rgba
                              length:(NSUInteger)overlay->camera_source_width *
                                     overlay->camera_source_height * 4
                             options:MTLResourceStorageModeShared];
    id<MTLBuffer> overlay_uniforms = [device newBufferWithBytes:overlay
                                                         length:sizeof(*overlay)
                                                        options:MTLResourceStorageModeShared];
    uint32_t source_dimensions[2] = {source_width, source_height};
    float time = (float)seconds;
    id<MTLCommandBuffer> commands = [queue commandBuffer];
    id<MTLComputeCommandEncoder> encoder = [commands computeCommandEncoder];
    [encoder setComputePipelineState:pipeline];
    [encoder setBuffer:source offset:0 atIndex:0];
    [encoder setBuffer:output offset:0 atIndex:1];
    [encoder setBuffer:uniforms offset:0 atIndex:2];
    [encoder setBytes:source_dimensions length:sizeof(source_dimensions) atIndex:3];
    [encoder setBytes:&time length:sizeof(time) atIndex:4];
    [encoder setBuffer:cursor offset:0 atIndex:5];
    [encoder setBuffer:camera offset:0 atIndex:6];
    [encoder setBuffer:overlay_uniforms offset:0 atIndex:7];
    MTLSize grid = MTLSizeMake(output_width, output_height, 1);
    NSUInteger width = MIN(pipeline.threadExecutionWidth, output_width);
    NSUInteger height = MIN(MAX((NSUInteger)1,
      pipeline.maxTotalThreadsPerThreadgroup / MAX(width, (NSUInteger)1)),
      output_height);
    [encoder dispatchThreads:grid threadsPerThreadgroup:MTLSizeMake(width, height, 1)];
    [encoder endEncoding];
    [commands commit];
    [commands waitUntilCompleted];
    if (commands.status == MTLCommandBufferStatusError) {
      return fail(error_text, error_capacity,
                  commands.error.localizedDescription ?:
                    @"The Metal still compositor failed");
    }
    memcpy(output_rgba, output.contents, output_length);
    return 1;
  }
}

int screenwide_gpu_alpha_composite(const uint8_t *base_rgba,
                                   const uint8_t *overlay_rgba,
                                   uint32_t width,
                                   uint32_t height,
                                   uint8_t *output_rgba,
                                   char *error_text,
                                   size_t error_capacity) {
  @autoreleasepool {
    if (base_rgba == NULL || overlay_rgba == NULL || output_rgba == NULL ||
        width == 0 || height == 0) {
      return fail(error_text, error_capacity,
                  @"The GPU layer compositor received invalid pixels");
    }
    static id<MTLDevice> device;
    static id<MTLComputePipelineState> pipeline;
    static id<MTLCommandQueue> queue;
    static NSString *initialization_error;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
      NSError *error = nil;
      device = MTLCreateSystemDefaultDevice();
      id<MTLLibrary> library = [device newLibraryWithSource:shader_source
                                                    options:nil
                                                      error:&error];
      id<MTLFunction> function =
          [library newFunctionWithName:@"alpha_composite_rgba"];
      pipeline = [device newComputePipelineStateWithFunction:function error:&error];
      queue = [device newCommandQueue];
      initialization_error = error.localizedDescription;
    });
    if (device == nil || pipeline == nil || queue == nil) {
      return fail(error_text, error_capacity,
                  initialization_error ?:
                    @"The Metal layer compositor could not be created");
    }
    NSUInteger pixel_count = (NSUInteger)width * height;
    NSUInteger byte_length = pixel_count * 4;
    id<MTLBuffer> base = [device newBufferWithBytes:base_rgba
                                             length:byte_length
                                            options:MTLResourceStorageModeShared];
    id<MTLBuffer> overlay = [device newBufferWithBytes:overlay_rgba
                                                length:byte_length
                                               options:MTLResourceStorageModeShared];
    id<MTLBuffer> output = [device newBufferWithLength:byte_length
                                               options:MTLResourceStorageModeShared];
    id<MTLCommandBuffer> commands = [queue commandBuffer];
    id<MTLComputeCommandEncoder> encoder = [commands computeCommandEncoder];
    [encoder setComputePipelineState:pipeline];
    [encoder setBuffer:base offset:0 atIndex:0];
    [encoder setBuffer:overlay offset:0 atIndex:1];
    [encoder setBuffer:output offset:0 atIndex:2];
    NSUInteger group_width = MIN(pipeline.threadExecutionWidth, pixel_count);
    [encoder dispatchThreads:MTLSizeMake(pixel_count, 1, 1)
        threadsPerThreadgroup:MTLSizeMake(MAX(group_width, (NSUInteger)1), 1, 1)];
    [encoder endEncoding];
    [commands commit];
    [commands waitUntilCompleted];
    if (commands.status == MTLCommandBufferStatusError) {
      return fail(error_text, error_capacity,
                  commands.error.localizedDescription ?:
                    @"The Metal layer compositor failed");
    }
    memcpy(output_rgba, output.contents, byte_length);
    return 1;
  }
}
