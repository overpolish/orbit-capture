// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#import <AVFoundation/AVFoundation.h>
#import <CoreVideo/CoreVideo.h>

@interface ScreenwidePreviewScrubber : NSObject
@property(nonatomic, strong) AVPlayer *player;
@property(nonatomic, strong) AVPlayerItemVideoOutput *output;
// The freshest frame the output vended, kept so repeated pulls at the same
// position stay on one image. The output only vends each rendered frame once;
// asking again returns the previous frame and reads as scrub flicker.
@property(nonatomic) CVPixelBufferRef lastPixels;
@property(nonatomic) CMTime lastVendTime;
@end

@implementation ScreenwidePreviewScrubber
- (void)dealloc {
  if (_lastPixels != NULL) CVPixelBufferRelease(_lastPixels);
}
@end

void *screenwide_preview_scrubber_create(const char *path, uint32_t width, uint32_t height) {
  if (path == NULL || width == 0 || height == 0) return NULL;
  @autoreleasepool {
    NSURL *url = [NSURL fileURLWithPath:[NSString stringWithUTF8String:path]];
    AVPlayerItem *item = [AVPlayerItem playerItemWithURL:url];
    NSDictionary *attributes = @{
      (NSString *)kCVPixelBufferPixelFormatTypeKey : @(kCVPixelFormatType_32BGRA),
      (NSString *)kCVPixelBufferWidthKey : @(width),
      (NSString *)kCVPixelBufferHeightKey : @(height),
      (NSString *)kCVPixelBufferMetalCompatibilityKey : @YES,
    };
    AVPlayerItemVideoOutput *output =
      [[AVPlayerItemVideoOutput alloc] initWithPixelBufferAttributes:attributes];
    output.suppressesPlayerRendering = YES;
    [item addOutput:output];
    ScreenwidePreviewScrubber *scrubber = [ScreenwidePreviewScrubber new];
    scrubber.output = output;
    scrubber.player = [AVPlayer playerWithPlayerItem:item];
    scrubber.player.muted = YES;
    scrubber.player.automaticallyWaitsToMinimizeStalling = NO;
    scrubber.lastPixels = NULL;
    scrubber.lastVendTime = kCMTimeInvalid;
    [scrubber.player pause];
    return (__bridge_retained void *)scrubber;
  }
}

void *screenwide_preview_scrubber_copy_frame(void *handle, int64_t milliseconds, int rough,
                                        uint32_t *out_width, uint32_t *out_height) {
  if (handle == NULL || out_width == NULL || out_height == NULL) return NULL;
  @autoreleasepool {
    ScreenwidePreviewScrubber *scrubber = (__bridge ScreenwidePreviewScrubber *)handle;
    CMTime time = CMTimeMake(MAX(milliseconds, 0), 1000);
    // The decoder is persistent, so exact frame seeks do not recreate the
    // asset/player. A broad tolerance collapses a timeline drag onto a few
    // keyframes, which feels like ping-pong rather than skimming.
    (void)rough;
    CMTime tolerance = kCMTimeZero;
    dispatch_semaphore_t completed = dispatch_semaphore_create(0);
    [scrubber.player.currentItem cancelPendingSeeks];
    [scrubber.player seekToTime:time
               toleranceBefore:tolerance
                toleranceAfter:tolerance
             completionHandler:^(__unused BOOL finished) {
               dispatch_semaphore_signal(completed);
             }];
    if (dispatch_semaphore_wait(completed,
          dispatch_time(DISPATCH_TIME_NOW, (int64_t)(2 * NSEC_PER_SEC))) != 0) return NULL;

    CMTime landed = scrubber.player.currentItem.currentTime;
    // A repeat of the frame that was already vended must return the exact
    // same image; pulling the output again would hand back an older frame
    // and the preview would flicker between the two.
    if (scrubber.lastPixels != NULL &&
        CMTimeCompare(scrubber.lastVendTime, landed) == 0) {
      CVPixelBufferRetain(scrubber.lastPixels);
      *out_width = (uint32_t)CVPixelBufferGetWidth(scrubber.lastPixels);
      *out_height = (uint32_t)CVPixelBufferGetHeight(scrubber.lastPixels);
      return scrubber.lastPixels;
    }
    CVPixelBufferRef pixels = NULL;
    CMTime display_time = kCMTimeInvalid;
    // `copyPixelBufferForItemTime:` returns whatever the renderer currently
    // holds - immediately after a seek that is often still the previous
    // frame. `hasNewPixelBufferForItemTime:` is the signal that the frame
    // for the landed time has actually been rendered, so wait for it. A
    // genuinely new frame is ready on the first poll in practice; a seek
    // that lands inside the already-vended source frame never raises the
    // flag, so with a cached image in hand only a few polls are worth it
    // before serving the cache.
    NSUInteger budget = scrubber.lastPixels != NULL ? 8 : 250;
    for (NSUInteger attempt = 0; attempt < budget && pixels == NULL; attempt++) {
      if ([scrubber.output hasNewPixelBufferForItemTime:landed]) {
        pixels = [scrubber.output copyPixelBufferForItemTime:landed
                                          itemTimeForDisplay:&display_time];
        break;
      }
      [NSThread sleepForTimeInterval:0.001];
    }
    if (pixels == NULL) {
      if (scrubber.lastPixels != NULL) {
        scrubber.lastVendTime = landed;
        CVPixelBufferRetain(scrubber.lastPixels);
        *out_width = (uint32_t)CVPixelBufferGetWidth(scrubber.lastPixels);
        *out_height = (uint32_t)CVPixelBufferGetHeight(scrubber.lastPixels);
        return scrubber.lastPixels;
      }
      pixels = [scrubber.output copyPixelBufferForItemTime:landed
                                        itemTimeForDisplay:&display_time];
      if (pixels == NULL) return NULL;
    } else {
    }
    if (scrubber.lastPixels != NULL) CVPixelBufferRelease(scrubber.lastPixels);
    CVPixelBufferRetain(pixels);
    scrubber.lastPixels = pixels;
    scrubber.lastVendTime = landed;
    size_t width = CVPixelBufferGetWidth(pixels);
    size_t height = CVPixelBufferGetHeight(pixels);
    *out_width = (uint32_t)width;
    *out_height = (uint32_t)height;
    return pixels;
  }
}

void screenwide_preview_pixel_buffer_release(void *pixels) {
  if (pixels != NULL) CVPixelBufferRelease((CVPixelBufferRef)pixels);
}

int screenwide_preview_scrubber_resize(void *handle, uint32_t width, uint32_t height) {
  if (handle == NULL || width == 0 || height == 0) return 0;
  @autoreleasepool {
    ScreenwidePreviewScrubber *scrubber = (__bridge ScreenwidePreviewScrubber *)handle;
    AVPlayerItem *item = scrubber.player.currentItem;
    if (item == nil) return 0;
    // Swapping the video output keeps the player, its item and the decode
    // pipeline warm; recreating the whole player costs hundreds of
    // milliseconds every time a pane's on-screen size changes.
    NSDictionary *attributes = @{
      (NSString *)kCVPixelBufferPixelFormatTypeKey : @(kCVPixelFormatType_32BGRA),
      (NSString *)kCVPixelBufferWidthKey : @(width),
      (NSString *)kCVPixelBufferHeightKey : @(height),
      (NSString *)kCVPixelBufferMetalCompatibilityKey : @YES,
    };
    AVPlayerItemVideoOutput *output =
      [[AVPlayerItemVideoOutput alloc] initWithPixelBufferAttributes:attributes];
    output.suppressesPlayerRendering = YES;
    if (scrubber.output != nil) [item removeOutput:scrubber.output];
    [item addOutput:output];
    scrubber.output = output;
    if (scrubber.lastPixels != NULL) {
      CVPixelBufferRelease(scrubber.lastPixels);
      scrubber.lastPixels = NULL;
    }
    scrubber.lastVendTime = kCMTimeInvalid;
    return 1;
  }
}

void screenwide_preview_scrubber_destroy(void *handle) {
  if (handle == NULL) return;
  ScreenwidePreviewScrubber *scrubber = (__bridge_transfer ScreenwidePreviewScrubber *)handle;
  [scrubber.player pause];
  [scrubber.player.currentItem cancelPendingSeeks];
}
