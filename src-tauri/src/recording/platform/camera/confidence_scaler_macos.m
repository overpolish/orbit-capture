// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#import <CoreVideo/CoreVideo.h>
#import <Foundation/Foundation.h>
#import <Metal/Metal.h>

// The recording bar shows a camera confidence thumbnail no larger than 48 × 30.
// Doing that on the CPU meant a full-frame format conversion plus a ColorSync
// colour match per frame, so the scale runs on the GPU instead: the camera
// pixel buffer is wrapped as Metal textures and a compute pass samples it
// straight into an RGBA readback buffer.
static NSString *const shader_source = @R"METAL(
#include <metal_stdlib>
using namespace metal;

struct ScaleUniforms {
  uint width;
  uint height;
  uint full_range;
};

constexpr sampler linear_sampler(coord::normalized, address::clamp_to_edge,
                                 filter::linear);

static uchar4 opaque_rgba(float3 rgb) {
  float3 scaled = saturate(rgb) * 255.0 + 0.5;
  return uchar4(uchar(scaled.r), uchar(scaled.g), uchar(scaled.b), 255);
}

static float3 yuv_to_rgb(float y, float2 uv, bool full_range) {
  float luma = full_range ? y : (y - 16.0 / 255.0) * (255.0 / 219.0);
  float2 chroma = full_range ? uv - 0.5 : (uv - 128.0 / 255.0) * (255.0 / 224.0);
  return clamp(float3(luma + 1.5748 * chroma.y,
                      luma - 0.1873 * chroma.x - 0.4681 * chroma.y,
                      luma + 1.8556 * chroma.x),
               0.0, 1.0);
}

kernel void scale_bgra(texture2d<float, access::sample> source [[texture(0)]],
                       device uchar4 *out [[buffer(0)]],
                       constant ScaleUniforms &u [[buffer(1)]],
                       uint2 position [[thread_position_in_grid]]) {
  if (position.x >= u.width || position.y >= u.height)
    return;
  float2 coordinate = (float2(position) + 0.5) / float2(u.width, u.height);
  float4 color = source.sample(linear_sampler, coordinate);
  out[position.y * u.width + position.x] = opaque_rgba(color.rgb);
}

kernel void scale_biplanar(texture2d<float, access::sample> luma [[texture(0)]],
                           texture2d<float, access::sample> chroma [[texture(1)]],
                           device uchar4 *out [[buffer(0)]],
                           constant ScaleUniforms &u [[buffer(1)]],
                           uint2 position [[thread_position_in_grid]]) {
  if (position.x >= u.width || position.y >= u.height)
    return;
  float2 coordinate = (float2(position) + 0.5) / float2(u.width, u.height);
  float3 rgb = yuv_to_rgb(luma.sample(linear_sampler, coordinate).r,
                          chroma.sample(linear_sampler, coordinate).rg,
                          u.full_range != 0);
  out[position.y * u.width + position.x] = opaque_rgba(rgb);
}
)METAL";

typedef struct {
  uint32_t width;
  uint32_t height;
  uint32_t full_range;
} ScreenwideScaleUniforms;

@interface ScreenwideConfidenceScaler : NSObject {
@public
  id<MTLDevice> device;
  id<MTLCommandQueue> queue;
  id<MTLComputePipelineState> bgra_pipeline;
  id<MTLComputePipelineState> biplanar_pipeline;
  id<MTLBuffer> readback;
  CVMetalTextureCacheRef texture_cache;
}
@end

@implementation ScreenwideConfidenceScaler
- (void)dealloc {
  if (texture_cache != NULL) {
    CVMetalTextureCacheFlush(texture_cache, 0);
    CFRelease(texture_cache);
    texture_cache = NULL;
  }
}
@end

static void report(char *error_text, size_t error_capacity, NSString *message) {
  if (error_text != NULL && error_capacity > 0) {
    snprintf(error_text, error_capacity, "%s",
             (message ?: @"The camera thumbnail scaler failed").UTF8String);
  }
}

static id<MTLTexture> plane_texture(CVMetalTextureCacheRef cache,
                                    CVPixelBufferRef pixels,
                                    MTLPixelFormat format, size_t width,
                                    size_t height, size_t plane,
                                    CVMetalTextureRef *reference) {
  CVReturn result = CVMetalTextureCacheCreateTextureFromImage(
      kCFAllocatorDefault, cache, pixels, NULL, format, width, height, plane,
      reference);
  if (result != kCVReturnSuccess || *reference == NULL)
    return nil;
  return CVMetalTextureGetTexture(*reference);
}

void *screenwide_confidence_scaler_create(char *error_text,
                                          size_t error_capacity) {
  @autoreleasepool {
    ScreenwideConfidenceScaler *scaler = [ScreenwideConfidenceScaler new];
    scaler->device = MTLCreateSystemDefaultDevice();
    if (scaler->device == nil) {
      report(error_text, error_capacity,
             @"This Mac has no Metal device for camera thumbnails");
      return NULL;
    }
    NSError *error = nil;
    id<MTLLibrary> library = [scaler->device newLibraryWithSource:shader_source
                                                          options:nil
                                                            error:&error];
    if (library == nil) {
      report(error_text, error_capacity, error.localizedDescription);
      return NULL;
    }
    scaler->bgra_pipeline = [scaler->device
        newComputePipelineStateWithFunction:[library
                                                newFunctionWithName:@"scale_bgra"]
                                      error:&error];
    scaler->biplanar_pipeline = [scaler->device
        newComputePipelineStateWithFunction:[library newFunctionWithName:
                                                         @"scale_biplanar"]
                                      error:&error];
    scaler->queue = [scaler->device newCommandQueue];
    // 48 × 30 RGBA is the largest thumbnail the recording bar asks for, so the
    // readback buffer is allocated once and only grows if that ever changes.
    scaler->readback =
        [scaler->device newBufferWithLength:48 * 30 * 4
                                    options:MTLResourceStorageModeShared];
    CVReturn cache = CVMetalTextureCacheCreate(
        kCFAllocatorDefault, NULL, scaler->device, NULL, &scaler->texture_cache);
    if (scaler->bgra_pipeline == nil || scaler->biplanar_pipeline == nil ||
        scaler->queue == nil || scaler->readback == nil ||
        cache != kCVReturnSuccess || scaler->texture_cache == NULL) {
      report(error_text, error_capacity,
             error.localizedDescription
                 ?: @"The camera thumbnail scaler could not be created");
      return NULL;
    }
    return (__bridge_retained void *)scaler;
  }
}

bool screenwide_confidence_scaler_thumbnail(void *handle,
                                            CVPixelBufferRef frame,
                                            uint16_t width, uint16_t height,
                                            uint8_t *out_rgba) {
  if (handle == NULL || frame == NULL || out_rgba == NULL || width == 0 ||
      height == 0)
    return false;
  @autoreleasepool {
    ScreenwideConfidenceScaler *scaler =
        (__bridge ScreenwideConfidenceScaler *)handle;
    size_t source_width = CVPixelBufferGetWidth(frame);
    size_t source_height = CVPixelBufferGetHeight(frame);
    if (source_width == 0 || source_height == 0)
      return false;

    OSType format = CVPixelBufferGetPixelFormatType(frame);
    bool biplanar = format == kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange ||
                    format == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange;
    if (!biplanar && format != kCVPixelFormatType_32BGRA)
      return false;

    size_t needed = (size_t)width * (size_t)height * 4;
    if (scaler->readback.length < needed) {
      scaler->readback =
          [scaler->device newBufferWithLength:needed
                                      options:MTLResourceStorageModeShared];
      if (scaler->readback == nil)
        return false;
    }

    CVMetalTextureRef first_ref = NULL;
    CVMetalTextureRef second_ref = NULL;
    id<MTLTexture> first = nil;
    id<MTLTexture> second = nil;
    bool wrapped = false;
    if (biplanar) {
      first = plane_texture(scaler->texture_cache, frame, MTLPixelFormatR8Unorm,
                            CVPixelBufferGetWidthOfPlane(frame, 0),
                            CVPixelBufferGetHeightOfPlane(frame, 0), 0,
                            &first_ref);
      second = plane_texture(scaler->texture_cache, frame, MTLPixelFormatRG8Unorm,
                             CVPixelBufferGetWidthOfPlane(frame, 1),
                             CVPixelBufferGetHeightOfPlane(frame, 1), 1,
                             &second_ref);
      wrapped = first != nil && second != nil;
    } else {
      first = plane_texture(scaler->texture_cache, frame,
                            MTLPixelFormatBGRA8Unorm, source_width,
                            source_height, 0, &first_ref);
      wrapped = first != nil;
    }

    bool ok = false;
    if (wrapped) {
      ScreenwideScaleUniforms uniforms = {
          .width = width,
          .height = height,
          .full_range =
              format == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange ? 1 : 0,
      };
      id<MTLComputePipelineState> pipeline =
          biplanar ? scaler->biplanar_pipeline : scaler->bgra_pipeline;
      id<MTLCommandBuffer> command = [scaler->queue commandBuffer];
      id<MTLComputeCommandEncoder> compute = [command computeCommandEncoder];
      [compute setComputePipelineState:pipeline];
      [compute setTexture:first atIndex:0];
      if (biplanar)
        [compute setTexture:second atIndex:1];
      [compute setBuffer:scaler->readback offset:0 atIndex:0];
      [compute setBytes:&uniforms length:sizeof(uniforms) atIndex:1];
      [compute dispatchThreads:MTLSizeMake(width, height, 1)
          threadsPerThreadgroup:MTLSizeMake(8, 8, 1)];
      [compute endEncoding];
      [command commit];
      [command waitUntilCompleted];
      if (command.status == MTLCommandBufferStatusCompleted) {
        memcpy(out_rgba, scaler->readback.contents, needed);
        ok = true;
      }
    }

    if (first_ref != NULL)
      CFRelease(first_ref);
    if (second_ref != NULL)
      CFRelease(second_ref);
    // The cache holds every texture it handed out until it is flushed, which is
    // what turned the old per-frame path into a leak.
    CVMetalTextureCacheFlush(scaler->texture_cache, 0);
    return ok;
  }
}

void screenwide_confidence_scaler_destroy(void *handle) {
  if (handle == NULL)
    return;
  @autoreleasepool {
    ScreenwideConfidenceScaler *scaler =
        (__bridge_transfer ScreenwideConfidenceScaler *)handle;
    (void)scaler;
  }
}
