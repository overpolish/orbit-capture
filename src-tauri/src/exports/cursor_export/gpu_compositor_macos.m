// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#import <AVFoundation/AVFoundation.h>
#import <Metal/Metal.h>
#import <VideoToolbox/VideoToolbox.h>

typedef bool (*OrbitShouldCancel)(void *context);
typedef void (*OrbitProgress)(void *context, uint64_t position_ms);

typedef struct {
  uint64_t frame;
  int x;
  int y;
} OrbitCursorPosition;

typedef struct {
  int32_t x;
  int32_t y;
  uint32_t cursor_width;
  uint32_t cursor_height;
  uint32_t output_width;
  uint32_t output_height;
} OrbitOverlayUniforms;

typedef struct {
  uint32_t crop_x;
  uint32_t crop_y;
  uint32_t crop_width;
  uint32_t crop_height;
  uint32_t frame_x;
  uint32_t frame_y;
  uint32_t frame_width;
  uint32_t frame_height;
  uint32_t radius;
} OrbitCameraOverlay;

typedef struct {
  uint32_t crop_x;
  uint32_t crop_y;
  uint32_t crop_width;
  uint32_t crop_height;
  uint32_t frame_x;
  uint32_t frame_y;
  uint32_t frame_width;
  uint32_t frame_height;
  uint32_t radius;
  uint32_t source_width;
  uint32_t source_height;
} OrbitCameraUniforms;

static NSString *const shader_source = @R"METAL(
#include <metal_stdlib>
using namespace metal;

struct OverlayUniforms {
  int x;
  int y;
  uint cursor_width;
  uint cursor_height;
  uint output_width;
  uint output_height;
};

struct CameraUniforms {
  uint crop_x;
  uint crop_y;
  uint crop_width;
  uint crop_height;
  uint frame_x;
  uint frame_y;
  uint frame_width;
  uint frame_height;
  uint radius;
  uint source_width;
  uint source_height;
};

static bool camera_pixel_visible(float2 point, constant CameraUniforms &u) {
  if (u.radius == 0) return true;
  float2 edge = min(point, float2(u.frame_width, u.frame_height) - point);
  float2 corner = max(float2(0.0), float(u.radius) - edge);
  return length(corner) <= float(u.radius);
}

static float4 camera_pixel(texture2d<float, access::sample> camera,
                           float2 point, constant CameraUniforms &u) {
  if (!camera_pixel_visible(point, u)) return float4(0.0);
  constexpr sampler linear_sampler(coord::normalized, address::clamp_to_edge,
                                   filter::linear);
  float2 source = float2(u.crop_x, u.crop_y) +
                  point * float2(u.crop_width, u.crop_height) /
                      float2(u.frame_width, u.frame_height);
  return float4(camera.sample(linear_sampler,
                              source / float2(u.source_width, u.source_height)).rgb,
                1.0);
}

kernel void overlay_camera_luma(
    texture2d<float, access::sample> camera [[texture(0)]],
    texture2d<float, access::read_write> luma [[texture(1)]],
    constant CameraUniforms &u [[buffer(0)]],
    uint2 gid [[thread_position_in_grid]]) {
  if (gid.x >= u.frame_width || gid.y >= u.frame_height) return;
  float4 rgba = camera_pixel(camera, float2(gid) + 0.5, u);
  if (rgba.a <= 0.0001) return;
  float camera_y = 16.0 / 255.0 +
                   dot(rgba.rgb, float3(0.182586, 0.614231, 0.062007));
  luma.write(camera_y, uint2(u.frame_x, u.frame_y) + gid);
}

kernel void overlay_camera_chroma(
    texture2d<float, access::sample> camera [[texture(0)]],
    texture2d<float, access::read_write> chroma [[texture(1)]],
    constant CameraUniforms &u [[buffer(0)]],
    uint2 gid [[thread_position_in_grid]]) {
  uint2 origin = gid * 2;
  if (origin.x >= u.frame_width || origin.y >= u.frame_height) return;
  float3 rgb_sum = 0.0;
  float alpha_sum = 0.0;
  for (uint y = 0; y < 2; ++y) {
    for (uint x = 0; x < 2; ++x) {
      float2 point = min(float2(origin + uint2(x, y)) + 0.5,
                         float2(u.frame_width, u.frame_height) - 0.5);
      float4 rgba = camera_pixel(camera, point, u);
      rgb_sum += rgba.rgb * rgba.a;
      alpha_sum += rgba.a;
    }
  }
  float alpha = alpha_sum * 0.25;
  if (alpha <= 0.0001) return;
  float3 rgb = rgb_sum / max(alpha_sum, 0.0001);
  float2 camera_uv = float2(
      0.5 + dot(rgb, float3(-0.100644, -0.338572, 0.439216)),
      0.5 + dot(rgb, float3(0.439216, -0.398942, -0.040274)));
  uint2 output = (uint2(u.frame_x, u.frame_y) + origin) / 2;
  float2 existing = chroma.read(output).rg;
  chroma.write(float4(mix(existing, camera_uv, alpha), 0.0, 1.0), output);
}

kernel void overlay_luma(texture2d<float, access::read> cursor [[texture(0)]],
                         texture2d<float, access::read_write> luma [[texture(1)]],
                         constant OverlayUniforms &u [[buffer(0)]],
                         uint2 gid [[thread_position_in_grid]]) {
  if (gid.x >= u.cursor_width || gid.y >= u.cursor_height) return;
  int2 output = int2(u.x, u.y) + int2(gid);
  if (output.x < 0 || output.y < 0 || output.x >= int(u.output_width) ||
      output.y >= int(u.output_height)) return;
  float4 rgba = cursor.read(gid);
  if (rgba.a <= 0.0001) return;
  float3 rgb = rgba.rgb;
  float cursor_y = 16.0 / 255.0 + dot(rgb, float3(0.182586, 0.614231, 0.062007));
  float existing = luma.read(uint2(output)).r;
  luma.write(mix(existing, cursor_y, rgba.a), uint2(output));
}

kernel void overlay_chroma(texture2d<float, access::read> cursor [[texture(0)]],
                           texture2d<float, access::read_write> chroma [[texture(1)]],
                           constant OverlayUniforms &u [[buffer(0)]],
                           uint2 gid [[thread_position_in_grid]]) {
  uint2 cursor_origin = gid * 2;
  if (cursor_origin.x >= u.cursor_width || cursor_origin.y >= u.cursor_height) return;
  int2 output_pixel = int2(u.x, u.y) + int2(cursor_origin);
  int2 output = output_pixel / 2;
  if (output.x < 0 || output.y < 0 || output.x >= int((u.output_width + 1) / 2) ||
      output.y >= int((u.output_height + 1) / 2)) return;
  float3 rgb_sum = 0.0;
  float alpha_sum = 0.0;
  for (uint y = 0; y < 2; ++y) {
    for (uint x = 0; x < 2; ++x) {
      uint2 point = min(cursor_origin + uint2(x, y),
                        uint2(u.cursor_width - 1, u.cursor_height - 1));
      float4 rgba = cursor.read(point);
      rgb_sum += rgba.rgb * rgba.a;
      alpha_sum += rgba.a;
    }
  }
  float alpha = alpha_sum * 0.25;
  if (alpha <= 0.0001) return;
  float3 rgb = rgb_sum / max(alpha_sum, 0.0001);
  float2 cursor_uv = float2(
      0.5 + dot(rgb, float3(-0.100644, -0.338572, 0.439216)),
      0.5 + dot(rgb, float3(0.439216, -0.398942, -0.040274)));
  float2 existing = chroma.read(uint2(output)).rg;
  chroma.write(float4(mix(existing, cursor_uv, alpha), 0.0, 1.0), uint2(output));
}
)METAL";

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

static NSArray<NSValue *> *read_positions(NSString *path, NSError **error) {
  NSString *contents = [NSString stringWithContentsOfFile:path
                                                 encoding:NSUTF8StringEncoding
                                                    error:error];
  if (contents == nil)
    return nil;
  NSMutableArray<NSValue *> *positions = [NSMutableArray array];
  [contents enumerateLinesUsingBlock:^(NSString *line, BOOL *stop) {
    (void)stop;
    double seconds = 0.0;
    OrbitCursorPosition position = {0, -100000, -100000};
    if (sscanf(line.UTF8String, "%lf overlay@cursor x %d, overlay@cursor y %d;",
               &seconds, &position.x, &position.y) == 3) {
      position.frame = (uint64_t)llround(seconds * 60.0);
      [positions
          addObject:[NSValue valueWithBytes:&position
                                   objCType:@encode(OrbitCursorPosition)]];
    }
  }];
  return positions;
}

static OrbitCursorPosition position_at(NSArray<NSValue *> *positions,
                                       NSUInteger *index, uint64_t frame) {
  while (*index + 1 < positions.count) {
    OrbitCursorPosition next;
    [positions[*index + 1] getValue:&next size:sizeof(next)];
    if (next.frame > frame)
      break;
    ++*index;
  }
  OrbitCursorPosition position = {0, -100000, -100000};
  if (positions.count > 0) {
    OrbitCursorPosition candidate;
    [positions[*index] getValue:&candidate size:sizeof(candidate)];
    if (candidate.frame <= frame)
      position = candidate;
  }
  return position;
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
      *error = [NSError errorWithDomain:@"OrbitCaptureGPUCompositor"
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

int orbit_gpu_composite_cursor(const char *screen_path, const char *cursor_path,
                               const char *commands_path,
                               const char *camera_path,
                               const OrbitCameraOverlay *camera_overlay,
                               const char *output_path, uint32_t source_width,
                               uint32_t source_height, uint32_t output_width,
                               uint32_t output_height, uint64_t bitrate,
                               void *context, OrbitShouldCancel should_cancel,
                               OrbitProgress progress, char *error_text,
                               size_t error_capacity) {
  (void)source_width;
  (void)source_height;
  @autoreleasepool {
    NSError *error = nil;
    AVURLAsset *screen_asset =
        [AVURLAsset URLAssetWithURL:[NSURL fileURLWithPath:@(screen_path)]
                            options:nil];
    AVURLAsset *cursor_asset =
        [AVURLAsset URLAssetWithURL:[NSURL fileURLWithPath:@(cursor_path)]
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
    AVAssetTrack *cursor_track = video_tracks(cursor_asset, &error).firstObject;
    AVAssetTrack *camera_track =
        camera_asset == nil ? nil : video_tracks(camera_asset, &error).firstObject;
    if (screen_track == nil || cursor_track == nil)
      return fail(error_text, error_capacity,
                  @"The GPU compositor could not find both video tracks");
    if (camera_asset != nil && camera_track == nil)
      return fail(error_text, error_capacity,
                  error.localizedDescription ?:
                    @"The GPU compositor could not find the camera track");
    NSArray<NSValue *> *positions = read_positions(@(commands_path), &error);
    if (positions == nil)
      return fail(error_text, error_capacity, error.localizedDescription);

    AVAssetReader *screen_reader =
        [[AVAssetReader alloc] initWithAsset:screen_asset error:&error];
    AVAssetReaderTrackOutput *screen_output =
        reader_output(screen_reader, screen_track,
                      kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
                      @(output_width), @(output_height), &error);
    if (screen_output == nil)
      return fail(error_text, error_capacity, error.localizedDescription);
    AVAssetReader *cursor_reader =
        [[AVAssetReader alloc] initWithAsset:cursor_asset error:&error];
    AVAssetReaderTrackOutput *cursor_output =
        reader_output(cursor_reader, cursor_track, kCVPixelFormatType_32BGRA,
                      nil, nil, &error);
    if (cursor_output == nil)
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
    NSDictionary *video_settings = @{
      AVVideoCodecKey : AVVideoCodecTypeH264,
      AVVideoWidthKey : @(output_width),
      AVVideoHeightKey : @(output_height),
      AVVideoCompressionPropertiesKey : @{
        AVVideoAverageBitRateKey : @(bitrate),
        AVVideoExpectedSourceFrameRateKey : @60,
        AVVideoMaxKeyFrameIntervalKey : @120,
        AVVideoAllowFrameReorderingKey : @YES,
        AVVideoProfileLevelKey : AVVideoProfileLevelH264HighAutoLevel,
        (__bridge NSString *)
        kVTCompressionPropertyKey_PrioritizeEncodingSpeedOverQuality : @YES,
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
    id<MTLCommandQueue> queue = [device newCommandQueue];
    CVMetalTextureCacheRef texture_cache = NULL;
    CVMetalTextureCacheCreate(kCFAllocatorDefault, NULL, device, NULL,
                              &texture_cache);
    if (device == nil || library == nil || luma_pipeline == nil ||
        chroma_pipeline == nil || camera_luma_pipeline == nil ||
        camera_chroma_pipeline == nil || queue == nil ||
        texture_cache == NULL)
      return fail(error_text, error_capacity,
                  error.localizedDescription
                      ?: @"The Metal cursor shader could not be created");

    if (![screen_reader startReading] || ![cursor_reader startReading] ||
        (camera_reader != nil && ![camera_reader startReading]) ||
        ![writer startWriting]) {
      CFRelease(texture_cache);
      return fail(error_text, error_capacity,
                  screen_reader.error.localizedDescription ?:
                    cursor_reader.error.localizedDescription ?:
                    camera_reader.error.localizedDescription ?:
                    writer.error.localizedDescription ?:
                    @"The GPU export could not be started");
    }
    [writer startSessionAtSourceTime:kCMTimeZero];
    CMSampleBufferRef cursor_sample = NULL;
    CMSampleBufferRef next_cursor_sample = [cursor_output copyNextSampleBuffer];
    CMSampleBufferRef camera_sample = NULL;
    CMSampleBufferRef next_camera_sample =
        [camera_output copyNextSampleBuffer];
    uint64_t cursor_frame = 0;
    uint64_t next_cursor_frame = 0;
    NSUInteger position_index = 0;
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
        while (next_cursor_sample != NULL &&
               CMTimeCompare(
                   CMSampleBufferGetPresentationTimeStamp(next_cursor_sample),
                   pts) <= 0) {
          if (cursor_sample != NULL)
            CFRelease(cursor_sample);
          cursor_sample = next_cursor_sample;
          next_cursor_sample = [cursor_output copyNextSampleBuffer];
          cursor_frame = next_cursor_frame;
          ++next_cursor_frame;
        }
        while (next_camera_sample != NULL &&
               CMTimeCompare(
                   CMSampleBufferGetPresentationTimeStamp(next_camera_sample),
                   pts) <= 0) {
          if (camera_sample != NULL)
            CFRelease(camera_sample);
          camera_sample = next_camera_sample;
          next_camera_sample = [camera_output copyNextSampleBuffer];
        }
        while (!writer_input.readyForMoreMediaData) {
          if (should_cancel != NULL && should_cancel(context)) {
            cancelled = true;
            break;
          }
          [NSThread sleepForTimeInterval:0.001];
        }
        if (cancelled) {
          CFRelease(screen_sample);
          break;
        }
        CVPixelBufferRef destination = NULL;
        if (CVPixelBufferPoolCreatePixelBuffer(
                kCFAllocatorDefault, adaptor.pixelBufferPool, &destination) !=
                kCVReturnSuccess ||
            destination == NULL) {
          CFRelease(screen_sample);
          error =
              [NSError errorWithDomain:@"OrbitCaptureGPUCompositor"
                                  code:2
                              userInfo:@{
                                NSLocalizedDescriptionKey :
                                    @"The GPU encoder ran out of video buffers"
                              }];
          break;
        }
        CVPixelBufferRef source = CMSampleBufferGetImageBuffer(screen_sample);
        size_t y_width = CVPixelBufferGetWidthOfPlane(source, 0);
        size_t y_height = CVPixelBufferGetHeightOfPlane(source, 0);
        size_t uv_width = CVPixelBufferGetWidthOfPlane(source, 1);
        size_t uv_height = CVPixelBufferGetHeightOfPlane(source, 1);
        CVMetalTextureRef source_y_ref = NULL, source_uv_ref = NULL;
        CVMetalTextureRef destination_y_ref = NULL, destination_uv_ref = NULL;
        id<MTLTexture> source_y =
            texture(texture_cache, source, MTLPixelFormatR8Unorm, y_width,
                    y_height, 0, &source_y_ref);
        id<MTLTexture> source_uv =
            texture(texture_cache, source, MTLPixelFormatRG8Unorm, uv_width,
                    uv_height, 1, &source_uv_ref);
        id<MTLTexture> destination_y =
            texture(texture_cache, destination, MTLPixelFormatR8Unorm, y_width,
                    y_height, 0, &destination_y_ref);
        id<MTLTexture> destination_uv =
            texture(texture_cache, destination, MTLPixelFormatRG8Unorm,
                    uv_width, uv_height, 1, &destination_uv_ref);
        id<MTLCommandBuffer> command = [queue commandBuffer];
        id<MTLBlitCommandEncoder> blit = [command blitCommandEncoder];
        [blit copyFromTexture:source_y
                  sourceSlice:0
                  sourceLevel:0
                 sourceOrigin:MTLOriginMake(0, 0, 0)
                   sourceSize:MTLSizeMake(y_width, y_height, 1)
                    toTexture:destination_y
             destinationSlice:0
             destinationLevel:0
            destinationOrigin:MTLOriginMake(0, 0, 0)];
        [blit copyFromTexture:source_uv
                  sourceSlice:0
                  sourceLevel:0
                 sourceOrigin:MTLOriginMake(0, 0, 0)
                   sourceSize:MTLSizeMake(uv_width, uv_height, 1)
                    toTexture:destination_uv
             destinationSlice:0
             destinationLevel:0
            destinationOrigin:MTLOriginMake(0, 0, 0)];
        [blit endEncoding];
        CVMetalTextureRef cursor_ref = NULL;
        if (cursor_sample != NULL) {
          CVPixelBufferRef cursor_pixels =
              CMSampleBufferGetImageBuffer(cursor_sample);
          size_t cursor_width = CVPixelBufferGetWidth(cursor_pixels);
          size_t cursor_height = CVPixelBufferGetHeight(cursor_pixels);
          id<MTLTexture> cursor_texture =
              texture(texture_cache, cursor_pixels, MTLPixelFormatBGRA8Unorm,
                      cursor_width, cursor_height, 0, &cursor_ref);
          OrbitCursorPosition position =
              position_at(positions, &position_index, cursor_frame);
          OrbitOverlayUniforms uniforms = {
              position.x,
              position.y,
              (uint32_t)cursor_width,
              (uint32_t)cursor_height,
              output_width,
              output_height,
          };
          MTLSize group = MTLSizeMake(16, 16, 1);
          id<MTLComputeCommandEncoder> compute =
              [command computeCommandEncoder];
          [compute setComputePipelineState:luma_pipeline];
          [compute setTexture:cursor_texture atIndex:0];
          [compute setTexture:destination_y atIndex:1];
          [compute setBytes:&uniforms length:sizeof(uniforms) atIndex:0];
          [compute dispatchThreads:MTLSizeMake(cursor_width, cursor_height, 1)
              threadsPerThreadgroup:group];
          [compute endEncoding];
          compute = [command computeCommandEncoder];
          [compute setComputePipelineState:chroma_pipeline];
          [compute setTexture:cursor_texture atIndex:0];
          [compute setTexture:destination_uv atIndex:1];
          [compute setBytes:&uniforms length:sizeof(uniforms) atIndex:0];
          [compute dispatchThreads:MTLSizeMake((cursor_width + 1) / 2,
                                               (cursor_height + 1) / 2, 1)
              threadsPerThreadgroup:group];
          [compute endEncoding];
        }
        CVMetalTextureRef camera_ref = NULL;
        if (camera_sample != NULL && camera_overlay != NULL) {
          CVPixelBufferRef camera_pixels =
              CMSampleBufferGetImageBuffer(camera_sample);
          size_t camera_width = CVPixelBufferGetWidth(camera_pixels);
          size_t camera_height = CVPixelBufferGetHeight(camera_pixels);
          id<MTLTexture> camera_texture =
              texture(texture_cache, camera_pixels, MTLPixelFormatBGRA8Unorm,
                      camera_width, camera_height, 0, &camera_ref);
          OrbitCameraUniforms camera_uniforms = {
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
              dispatchThreads:MTLSizeMake(camera_overlay->frame_width,
                                          camera_overlay->frame_height, 1)
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
              dispatchThreads:MTLSizeMake(
                                  (camera_overlay->frame_width + 1) / 2,
                                  (camera_overlay->frame_height + 1) / 2, 1)
              threadsPerThreadgroup:camera_group];
          [camera_compute endEncoding];
        }
        [command commit];
        [command waitUntilCompleted];
        if (command.status == MTLCommandBufferStatusError ||
            ![adaptor appendPixelBuffer:destination withPresentationTime:pts]) {
          error = command.error ?: writer.error ?:
              [NSError errorWithDomain:@"OrbitCaptureGPUCompositor"
                                  code:3
                              userInfo:@{NSLocalizedDescriptionKey :
                                           @"The GPU encoder rejected a video frame"}];
        }
        if (cursor_ref != NULL)
          CFRelease(cursor_ref);
        if (camera_ref != NULL)
          CFRelease(camera_ref);
        CFRelease(source_y_ref);
        CFRelease(source_uv_ref);
        CFRelease(destination_y_ref);
        CFRelease(destination_uv_ref);
        CVPixelBufferRelease(destination);
        CFRelease(screen_sample);
        if (error != nil)
          break;
        if (progress != NULL)
          progress(context, (uint64_t)llround(CMTimeGetSeconds(pts) * 1000.0));
      }
    }
    if (cursor_sample != NULL)
      CFRelease(cursor_sample);
    if (next_cursor_sample != NULL)
      CFRelease(next_cursor_sample);
    if (camera_sample != NULL)
      CFRelease(camera_sample);
    if (next_camera_sample != NULL)
      CFRelease(next_camera_sample);
    CFRelease(texture_cache);
    if (cancelled) {
      [screen_reader cancelReading];
      [cursor_reader cancelReading];
      [camera_reader cancelReading];
      [writer cancelWriting];
      [[NSFileManager defaultManager] removeItemAtURL:output_url error:nil];
      return -1;
    }
    if (error != nil || screen_reader.status == AVAssetReaderStatusFailed ||
        cursor_reader.status == AVAssetReaderStatusFailed ||
        camera_reader.status == AVAssetReaderStatusFailed) {
      [writer cancelWriting];
      [[NSFileManager defaultManager] removeItemAtURL:output_url error:nil];
      return fail(error_text, error_capacity,
                  error.localizedDescription ?:
                    screen_reader.error.localizedDescription ?:
                    cursor_reader.error.localizedDescription ?:
                    camera_reader.error.localizedDescription ?:
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
