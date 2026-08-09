// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

/** Close enough to the end that pressing play means "again from the top". */
export const END_TOLERANCE_SECONDS = 0.02;
/**
 * `HTMLMediaElement.HAVE_FUTURE_DATA`: there is enough decoded to keep going
 * from where the element is now. Below it the element is not moving, whatever
 * it was last asked to do.
 */
export const HAVE_FUTURE_DATA = 3;
/**
 * How long the picture is held after a press so that sound and picture start
 * together.
 *
 * A media element's audio renderer starts from cold on every transition out of
 * pause. Measured in a WKWebView against a preview mix: `play()` has the
 * picture moving within ~50ms, but the first sample reaches the output only
 * 290-370ms later, and WebKit then pulls the picture back to the audio clock -
 * which is the moment of frozen video and stuck scrubber a person sees a beat
 * after pressing play.
 *
 * Warming the renderer up in advance is not possible. Measured, all of it: the
 * warmth is gone within 250ms of a pause, so warming on a scrub does nothing;
 * a muted play does not produce it; `playbackRate` below 1 suppresses the audio
 * entirely and starts the wait again on release; pinning `currentTime` each
 * frame stops the renderer ever starting; and a second element already playing
 * does not warm this one. So the picture waits instead of the sound rushing to
 * catch up.
 *
 * The wait is spent playing, silently, from a little *before* the position that
 * was pressed, so that the moment the picture is revealed is the moment it
 * arrives at that position. Correcting afterwards with a seek was tried and
 * cannot work: any seek during playback restarts the renderer, which is the
 * silence this exists to remove - a 0.05s nudge cost 300ms of sound, a 0.26s
 * one cost 470ms.
 */
export const PREROLL_SECONDS = 0.28;
/**
 * How long a start is held at the very least.
 *
 * The pre-roll is what normally decides the reveal - the position reaching the
 * press is the last condition to come true - so this only matters for a press
 * close enough to the beginning that there is no room to roll from. Measured:
 * with a full pre-roll the reveal lands at 481-533ms and the renderer has been
 * running for a while by then.
 */
export const MIN_HOLD_MS = 480;
/** The first start of a newly loaded file is slower: measured 480-500ms cold. */
export const FIRST_PLAY_MIN_HOLD_MS = 700;
/** However confused the element gets, nothing stays hidden longer than this. */
export const HOLD_TIMEOUT_MS = 2000;
/** And nothing waits longer than this for a replacement file to show a frame. */
export const SWAP_TIMEOUT_MS = 4000;

/**
 * Why the preview is being held, if it is.
 *
 * There is one set of these and everything the user sees is a projection of
 * it. It was three independent booleans first - one per reason, each owned by
 * whichever piece of code happened to know about it - and a single toggle then
 * showed and hid the loader three or four times, because nothing made the
 * three agree about when the operation started and when it ended. A refcounted
 * set has exactly one moment where it stops being empty and one where it
 * becomes empty again, and those are the only two transitions the eye sees.
 */
export type HoldReason =
  /** A new mix is being built for tracks the user just changed. */
  | "remix"
  /** The element is being pointed at a different file. */
  | "swap"
  /** A press is waiting for the audio renderer to come up. */
  | "start";

/**
 * Whether the reason needs the picture on screen kept still.
 *
 * A remix does not: FFmpeg is writing a new file somewhere else entirely and
 * whatever is playing now keeps playing. A swap and a start both do - the
 * element is either reloading or running out of sight.
 */
export const HOLDS_FRAME: Record<HoldReason, boolean> = {
  remix: false,
  start: true,
  swap: true,
};

/**
 * Whether the reason is worth saying out loud.
 *
 * A start is not: the frame is held for well under a second and the held frame
 * is its own affordance - a spinner thrown over the picture for that long
 * reads as a fault rather than as a press being answered. A remix and a swap
 * are, because between them they can take as long as FFmpeg does.
 */
export const SHOWS_LOADER: Record<HoldReason, boolean> = {
  remix: true,
  start: false,
  swap: true,
};

/** How many of the current reasons want the picture kept still. */
export const countFrameHolds = (reasons: ReadonlySet<HoldReason>) => {
  let count = 0;
  for (const reason of reasons) if (HOLDS_FRAME[reason]) count += 1;
  return count;
};
