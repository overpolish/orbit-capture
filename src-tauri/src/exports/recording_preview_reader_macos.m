// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#import <AVFoundation/AVFoundation.h>
#import <CoreMedia/CoreMedia.h>

void screenwide_preview_reader_enable_random_access(void *output_handle) {
  if (output_handle == NULL) return;
  AVAssetReaderOutput *output = (__bridge AVAssetReaderOutput *)output_handle;
  output.supportsRandomAccess = YES;
}

int screenwide_preview_reader_reset_range(void *output_handle,
                                     int64_t start_milliseconds,
                                     int64_t duration_milliseconds) {
  if (output_handle == NULL) return 0;
  AVAssetReaderOutput *output = (__bridge AVAssetReaderOutput *)output_handle;
  CMTimeRange range = CMTimeRangeMake(
      CMTimeMake(start_milliseconds, 1000),
      CMTimeMake(duration_milliseconds, 1000));
  // AVFoundation reports invalid random-access state with an NSException,
  // which must never cross the Rust FFI boundary. A failed reset is recoverable:
  // the caller can replace this reader while keeping the preview session alive.
  @try {
    [output resetForReadingTimeRanges:@[[NSValue valueWithCMTimeRange:range]]];
    return 1;
  } @catch (__unused NSException *exception) {
    return 0;
  }
}
