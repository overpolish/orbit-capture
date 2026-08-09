/**
 * Tracing for the export preview, kept in the product on purpose.
 *
 * The fit and the audio both depend on state the webview reveals only as media
 * data arrives, which is exactly the part that differs between a machine that
 * works and one that does not. Every decision point announces itself under one
 * prefix so a report can be reproduced, filtered and pasted back verbatim.
 */
const PREFIX = "[export-preview]";

export const logPreview = (
  event: string,
  detail: Record<string, unknown> = {},
) => {
  console.log(`${PREFIX} ${event}`, JSON.stringify(detail));
};

/** What a media element will admit to at a given moment. */
export const describeMedia = (
  media: HTMLMediaElement | HTMLImageElement | null,
): Record<string, unknown> => {
  if (!media) return { present: false };

  const shared = {
    // The laid-out size, which is what the fit scale actually multiplies. It
    // can disagree with the intrinsic size the media reports.
    layoutHeight: media.offsetHeight,
    layoutWidth: media.offsetWidth,
    present: true,
  };

  if (media instanceof HTMLVideoElement) {
    return {
      ...shared,
      currentTime: media.currentTime,
      duration: media.duration,
      error: media.error
        ? `${media.error.code.toString()}: ${media.error.message}`
        : null,
      muted: media.muted,
      networkState: media.networkState,
      paused: media.paused,
      readyState: media.readyState,
      videoHeight: media.videoHeight,
      videoWidth: media.videoWidth,
    };
  }
  if (media instanceof HTMLAudioElement) {
    return {
      ...shared,
      error: media.error
        ? `${media.error.code.toString()}: ${media.error.message}`
        : null,
      muted: media.muted,
      networkState: media.networkState,
      paused: media.paused,
      readyState: media.readyState,
      src: media.currentSrc || media.src,
      volume: media.volume,
    };
  }

  if (media instanceof HTMLImageElement) {
    return {
      ...shared,
      complete: media.complete,
      naturalHeight: media.naturalHeight,
      naturalWidth: media.naturalWidth,
    };
  }

  return shared;
};
