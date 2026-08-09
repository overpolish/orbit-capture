import { Check, Mic, Pause, Play, Volume2 } from "lucide-react";
import {
  PointerEvent as ReactPointerEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { Button } from "../../../components/base/button/button";
import { ToggleButton } from "../../../components/base/button/toggle-button";
import { CircularProgressBar } from "../../../components/base/circular-progress-bar/circular-progress-bar";
import { describeMedia, logPreview } from "../diagnostics";
import { formatDuration } from "../duration";
import { PreparedAudioTrack } from "../types";

import {
  FrozenFrame,
  PreviewViewport,
  VideoPreviewViewport,
} from "./preview-viewport";

export type RecordingMetadata = {
  durationMs: number;
  height: number;
  width: number;
};

type ScrubPreviewProps = {
  artifactId: number;
  durationMs: number;
  audioError?: string | null;
  audioTracks?: PreparedAudioTrack[];
  isPreparingAudio?: boolean;
  /** True while a new mix is being built for tracks the user just changed. */
  isRemixing?: boolean;
  /**
   * The muxed file to play, once there is one. Until then playback falls back
   * to `videoUrl`, which the window supplies only when the recording can be
   * heard in full without mixing.
   */
  mixUrl?: string | null;
  onEnabledTracksChange?: (streamIndices: number[]) => void;
  onMetadata?: (metadata: RecordingMetadata) => void;
  posterUrl?: string | null;
  videoUrl?: string | null;
};

const EMPTY_AUDIO_TRACKS: PreparedAudioTrack[] = [];

/** Close enough to the end that pressing play means "again from the top". */
const END_TOLERANCE_SECONDS = 0.02;
/**
 * `HTMLMediaElement.HAVE_FUTURE_DATA`: there is enough decoded to keep going
 * from where the element is now. Below it the element is not moving, whatever
 * it was last asked to do.
 */
const HAVE_FUTURE_DATA = 3;
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
const PREROLL_SECONDS = 0.28;
/**
 * How long a start is held at the very least.
 *
 * The pre-roll is what normally decides the reveal - the position reaching the
 * press is the last condition to come true - so this only matters for a press
 * close enough to the beginning that there is no room to roll from. Measured:
 * with a full pre-roll the reveal lands at 481-533ms and the renderer has been
 * running for a while by then.
 */
const MIN_HOLD_MS = 480;
/** The first start of a newly loaded file is slower: measured 480-500ms cold. */
const FIRST_PLAY_MIN_HOLD_MS = 700;
/** However confused the element gets, nothing stays hidden longer than this. */
const HOLD_TIMEOUT_MS = 2000;
/** And nothing waits longer than this for a replacement file to show a frame. */
const SWAP_TIMEOUT_MS = 4000;

const clamp = (value: number, min: number, max: number) =>
  Math.min(Math.max(value, min), max);

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
type HoldReason =
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
const HOLDS_FRAME: Record<HoldReason, boolean> = {
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
const SHOWS_LOADER: Record<HoldReason, boolean> = {
  remix: true,
  start: false,
  swap: true,
};

/** How many of the current reasons want the picture kept still. */
const countFrameHolds = (reasons: ReadonlySet<HoldReason>) => {
  let count = 0;
  for (const reason of reasons) if (HOLDS_FRAME[reason]) count += 1;
  return count;
};

type PlayheadListener = (seconds: number, ratio: number) => void;
type Playhead = ReturnType<typeof createPlayhead>;

/**
 * The playing position, deliberately kept out of React.
 *
 * Playback moves the position sixty times a second. Held in state, every one
 * of those became a render of this whole subtree - including the viewport,
 * which re-measures and re-applies its fit on every render - so the cost of
 * drawing a moving line was paid across components that had nothing to do
 * with it. Subscribers here write the new position straight to their own
 * element, the same way the viewport already drives its zoom.
 */
const createPlayhead = () => {
  const listeners = new Set<PlayheadListener>();
  let last = { ratio: 0, seconds: 0 };

  return {
    publish: (seconds: number, ratio: number) => {
      last = { ratio, seconds };
      for (const listener of listeners) listener(seconds, ratio);
    },
    subscribe: (listener: PlayheadListener) => {
      listeners.add(listener);
      // So a subscriber that mounts mid-playback is not left at zero.
      listener(last.seconds, last.ratio);
      return () => {
        listeners.delete(listener);
      };
    },
  };
};

const waveformPath = (points: number[]) => {
  if (points.length === 0) return "";
  const center = 20;
  const scale = 17;
  return points
    .map((peak, index) => {
      const x = (index / Math.max(1, points.length - 1)) * 1000;
      const height = Math.max(0.75, peak * scale);
      return `M${x.toFixed(2)} ${(center - height).toFixed(2)}V${(center + height).toFixed(2)}`;
    })
    .join(" ");
};

function Waveform({
  enabled,
  onSeek,
  playhead,
  track,
}: {
  enabled: boolean;
  onSeek: (ratio: number) => void;
  playhead: Playhead;
  track: PreparedAudioTrack;
}) {
  const path = useMemo(() => waveformPath(track.waveform), [track.waveform]);
  const lineRef = useRef<HTMLDivElement>(null);

  useEffect(
    () =>
      playhead.subscribe((_seconds, ratio) => {
        if (lineRef.current)
          lineRef.current.style.left = `${(ratio * 100).toString()}%`;
      }),
    [playhead],
  );

  const seek = (event: ReactPointerEvent<HTMLDivElement>) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    onSeek(clamp((event.clientX - bounds.left) / bounds.width, 0, 1));
  };

  return (
    <div
      className="relative h-6 min-w-0 grow cursor-ew-resize overflow-hidden rounded bg-muted/8"
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture(event.pointerId);
        seek(event);
      }}
      onPointerMove={(event) => {
        if (event.currentTarget.hasPointerCapture(event.pointerId)) seek(event);
      }}
      onPointerUp={(event) => {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }}
    >
      <svg
        aria-hidden="true"
        className={enabled ? "size-full text-info" : "size-full text-muted/35"}
        preserveAspectRatio="none"
        viewBox="0 0 1000 40"
      >
        <path
          className="stroke-current"
          d={path}
          fill="none"
          strokeWidth="2"
          vectorEffect="non-scaling-stroke"
        />
      </svg>
      <div
        className="pointer-events-none absolute inset-y-0 w-px bg-content-fg/80"
        ref={lineRef}
        style={{ left: "0%" }}
      />
    </div>
  );
}

function Timeline({
  onSeek,
  playhead,
}: {
  onSeek: (ratio: number) => void;
  playhead: Playhead;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  const fillRef = useRef<HTMLDivElement>(null);
  const knobRef = useRef<HTMLDivElement>(null);
  const ratioRef = useRef(0);

  useEffect(
    () =>
      playhead.subscribe((_seconds, ratio) => {
        ratioRef.current = ratio;
        const percent = `${(ratio * 100).toString()}%`;
        if (fillRef.current) fillRef.current.style.width = percent;
        if (knobRef.current) knobRef.current.style.left = percent;
        // Assistive technology needs the position too, and this element is
        // never re-rendered, so React will not overwrite the attribute.
        rootRef.current?.setAttribute(
          "aria-valuenow",
          Math.round(ratio * 100).toString(),
        );
      }),
    [playhead],
  );

  const seek = (event: ReactPointerEvent<HTMLDivElement>) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    onSeek(clamp((event.clientX - bounds.left) / bounds.width, 0, 1));
  };

  return (
    <div
      aria-label="Recording position"
      aria-valuemax={100}
      aria-valuemin={0}
      aria-valuenow={0}
      className="relative h-6 min-w-0 grow cursor-ew-resize touch-none"
      onKeyDown={(event) => {
        if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
        event.preventDefault();
        onSeek(
          clamp(
            ratioRef.current + (event.key === "ArrowRight" ? 0.01 : -0.01),
            0,
            1,
          ),
        );
      }}
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture(event.pointerId);
        seek(event);
      }}
      onPointerMove={(event) => {
        if (event.currentTarget.hasPointerCapture(event.pointerId)) seek(event);
      }}
      onPointerUp={(event) => {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }}
      ref={rootRef}
      role="slider"
      tabIndex={0}
    >
      <div className="absolute inset-x-0 top-1/2 h-1.5 -translate-y-1/2 overflow-hidden rounded-full bg-muted/15">
        <div
          className="h-full rounded-full bg-info"
          ref={fillRef}
          style={{ width: "0%" }}
        />
      </div>
      <div
        className="pointer-events-none absolute top-1/2 size-3 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-content bg-info shadow-sm"
        ref={knobRef}
        style={{ left: "0%" }}
      />
    </div>
  );
}

/** The elapsed half of the time readout, written straight to the text node. */
function ElapsedTime({ playhead }: { playhead: Playhead }) {
  const ref = useRef<HTMLSpanElement>(null);

  useEffect(
    () =>
      playhead.subscribe((seconds) => {
        const text = formatDuration(seconds * 1000);
        if (ref.current && ref.current.textContent !== text)
          ref.current.textContent = text;
      }),
    [playhead],
  );

  return <span ref={ref}>{formatDuration(0)}</span>;
}

/**
 * Playback-only verification for a finished recording.
 *
 * There is exactly one media element, and it plays one file that already
 * contains the tracks the user has switched on. Everything on screen - the
 * timeline, the waveforms, the readout - follows that element's clock.
 *
 * It was built the other way first: the recording playing muted alongside one
 * `<audio>` element per extracted track, nudged back into line whenever they
 * drifted. That cannot be made to work. Setting `currentTime` on a media
 * element is a seek; a seek silences it until its decoder catches up; the
 * video never stops advancing meanwhile, so the gap re-opens and the next
 * correction cancels the last. Muxing the selection into one file up front
 * removes the second clock rather than trying to chase it.
 *
 * There are deliberately no trim handles or editable ranges here.
 */
export function ScrubPreview({
  artifactId,
  audioError,
  audioTracks = EMPTY_AUDIO_TRACKS,
  durationMs,
  isPreparingAudio = false,
  isRemixing = false,
  mixUrl,
  onEnabledTracksChange,
  onMetadata,
  posterUrl,
  videoUrl,
}: ScrubPreviewProps) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const totalSecondsRef = useRef(0);
  /** The last position published, so a source swap can be resumed from it. */
  const positionRef = useRef(0);
  const isPlayingRef = useRef(false);
  const frozenFrameRef = useRef<FrozenFrame | null>(null);
  /**
   * Every reason the preview is currently held, refcounted.
   *
   * A start and a change of file both need the picture to stop where it is,
   * and they overlap: swapping in a new mix while playing does both at once.
   * Whoever asks first takes the snapshot, and the frame is only let go once
   * nobody still needs it - so the reasons compose instead of one clearing the
   * other's frame out from under it.
   *
   * Kept in a ref as well as in state because the effects below add and remove
   * reasons within a single flush and have to see each other's work
   * immediately; the state copy exists only so that rendering can follow.
   */
  const holdsRef = useRef(new Set<HoldReason>());
  /**
   * True from the moment a swap is armed until the new file has settled.
   *
   * `setMixUrl` and `setIsRemixing(false)` land in the same commit, so without
   * this the remix hold would be released in the same flush that arms the
   * swap, and React would paint the gap between them - the loader blinking off
   * and straight back on for one frame. This is how "remix" knows it is being
   * handed on rather than let go.
   */
  const isSwapPendingRef = useRef(false);
  /**
   * `isRemixing` where `settle` can read it.
   *
   * A settle is the end of *this* regeneration. Somebody who toggles another
   * track while the replacement is still loading has already started the next
   * one, and that build's hold is not this swap's to release.
   */
  const isRemixingRef = useRef(false);
  /** True between a press and the moment sound and picture are let go. */
  const isHeldRef = useRef(false);
  const holdFrameRef = useRef<number | undefined>(undefined);
  /** Whether this file has been played once, which is what makes it quicker. */
  const hasPlayedRef = useRef(false);
  const [playhead] = useState(createPlayhead);
  const [knownDurationMs, setKnownDurationMs] = useState(durationMs);
  const [isPlaying, setIsPlaying] = useState(false);
  const [hasFailed, setHasFailed] = useState(false);
  const [playbackError, setPlaybackError] = useState<string | null>(null);
  /** The render-visible copy of `holdsRef`, as an array so identity changes. */
  const [holdReasons, setHoldReasons] = useState<HoldReason[]>([]);
  const [enabledTracks, setEnabledTracks] = useState<Set<number>>(
    () => new Set(audioTracks.map((track) => track.streamIndex)),
  );

  const totalSeconds = Math.max(0, knownDurationMs / 1000);
  // The mix once it exists, and whatever the window considers playable until
  // then - which for a recording carrying more than one audio track is
  // nothing, because a media element renders only the first of them. See
  // `recordingPlaysUnmixed` in `export-window.tsx`.
  const source = mixUrl ?? videoUrl;
  // What the element is actually pointed at, which lags `source` by exactly
  // one commit: the frame on screen is captured first, so the element never
  // blanks between letting go of one file and painting the next.
  const [renderedSource, setRenderedSource] = useState(source);
  const showsVideo = Boolean(renderedSource) && !hasFailed;

  // Keyed on what the tracks *are* rather than on the array's identity, so a
  // rebuilt-but-identical list does not reset the user's choices.
  const trackKey = audioTracks.map((track) => track.streamIndex).join("-");

  useEffect(() => {
    // A new prepared result starts with every recorded track included.
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setEnabledTracks(new Set(trackKey ? trackKey.split("-").map(Number) : []));
  }, [trackKey]);

  // What the rows are set to, as the recorded track numbers a mix is asked
  // for by. A string so this is a stable dependency: the array it describes is
  // rebuilt on every render and would retrigger the report endlessly.
  const enabledSignature = audioTracks
    .filter((track) => enabledTracks.has(track.streamIndex))
    .map((track) => track.streamIndex)
    .join("-");
  const hasTracks = audioTracks.length > 0;

  useEffect(() => {
    // Before the tracks are prepared there is nothing to have an opinion
    // about, and reporting an empty selection then would be read as "the user
    // switched everything off".
    if (!hasTracks) return;
    const streamIndices = enabledSignature
      ? enabledSignature.split("-").map(Number)
      : [];
    logPreview("tracks.enabled", { streamIndices });
    onEnabledTracksChange?.(streamIndices);
  }, [enabledSignature, hasTracks, onEnabledTracksChange]);

  /** Pushes a position to whatever is drawing it, without rendering anything. */
  const publish = useCallback(
    (seconds: number) => {
      const total = totalSecondsRef.current;
      positionRef.current = seconds;
      playhead.publish(seconds, total > 0 ? clamp(seconds / total, 0, 1) : 0);
    },
    [playhead],
  );

  useEffect(() => {
    totalSecondsRef.current = totalSeconds;
    // Not while a file is being swapped in. A recovered recording arrives with
    // `durationMs` of 0, so every reload discovers the duration again and
    // changes `totalSeconds` - and the element it would be read from at that
    // moment is the replacement, sitting at 0, which would wipe out the
    // position the swap is in the middle of restoring.
    if (isSwapPendingRef.current) return;
    publish(videoRef.current?.currentTime ?? 0);
  }, [publish, totalSeconds]);

  /**
   * Adds a reason to hold the preview, taking the snapshot only if nobody else
   * already has: a second reason must not recapture, because by then the
   * element may be showing a file that is on its way out.
   *
   * Stable, and idempotent for a reason already on the list, so the effects
   * that call it can do so freely without either re-running or re-rendering.
   */
  const freeze = useCallback((reason: HoldReason) => {
    if (holdsRef.current.has(reason)) return;
    if (HOLDS_FRAME[reason] && countFrameHolds(holdsRef.current) === 0)
      frozenFrameRef.current?.capture();
    holdsRef.current.add(reason);
    // The point of this component: the loader follows the list of reasons, and
    // the list only changes when a reason genuinely arrives or leaves.
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setHoldReasons([...holdsRef.current]);
  }, []);

  /** Drops a reason, handing the picture back once it was the last one. */
  const thaw = useCallback((reason: HoldReason) => {
    if (!holdsRef.current.has(reason)) return;
    holdsRef.current.delete(reason);
    if (HOLDS_FRAME[reason] && countFrameHolds(holdsRef.current) === 0)
      frozenFrameRef.current?.clear();
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setHoldReasons([...holdsRef.current]);
  }, []);

  /** Whether anything on the list is worth putting a loader on screen for. */
  const isBusy = holdReasons.some((reason) => SHOWS_LOADER[reason]);

  /**
   * Lets a held start go, whether it ran its course or was interrupted.
   *
   * Everything it undoes is idempotent, so pausing, scrubbing, swapping the
   * file or unmounting during a hold all end in the same state: full volume,
   * no held frame, and a playhead back on the element's own clock.
   */
  const endHold = useCallback(
    // The position is published by default, because normally the element is
    // still pointed at the file whose clock the scrubber is following. A swap
    // is the exception: by the time this runs React has already repointed
    // `<video>` at the replacement, so `currentTime` is 0 and publishing it
    // would snap the scrubber and every waveform playhead to the left edge on
    // every toggle. The swap restores the position itself, from `positionRef`.
    ({ publish: shouldPublish = true }: { publish?: boolean } = {}) => {
      if (holdFrameRef.current !== undefined) {
        cancelAnimationFrame(holdFrameRef.current);
        holdFrameRef.current = undefined;
      }
      if (!isHeldRef.current) return;

      isHeldRef.current = false;
      thaw("start");
      const video = videoRef.current;
      if (!video) return;
      video.volume = 1;
      if (shouldPublish) publish(video.currentTime);
      logPreview("play.revealed", { position: video.currentTime });
    },
    [publish, thaw],
  );

  useEffect(() => endHold, [endHold]);

  /**
   * Everything a start needs before `play()` is called, so that nothing on
   * screen moves until there is sound to move with.
   */
  const startHeld = useCallback(
    (video: HTMLVideoElement) => {
      const target = video.currentTime;
      const floor = hasPlayedRef.current ? MIN_HOLD_MS : FIRST_PLAY_MIN_HOLD_MS;
      hasPlayedRef.current = true;
      isHeldRef.current = true;
      freeze("start");
      video.volume = 0;
      // The one seek involved, taken while the element is still paused - the
      // only kind that does not cost the renderer anything.
      video.currentTime = Math.max(0, target - PREROLL_SECONDS);
      logPreview("play.held", { floor, target });

      const startedAt = performance.now();
      const tick = () => {
        holdFrameRef.current = undefined;
        if (!isHeldRef.current) return;
        const held = videoRef.current;
        const elapsed = performance.now() - startedAt;
        // Revealed when the picture arrives back at the position that was
        // pressed, never before the renderer can have started, and never later
        // than the timeout however stuck the element is.
        if (
          !held ||
          elapsed > HOLD_TIMEOUT_MS ||
          (elapsed >= floor && held.currentTime >= target)
        ) {
          endHold();
          return;
        }
        holdFrameRef.current = requestAnimationFrame(tick);
      };
      holdFrameRef.current = requestAnimationFrame(tick);
    },
    [endHold, freeze],
  );

  /** Follows the one clock there is, for as long as it is running. */
  useEffect(() => {
    if (!isPlaying) return;
    let frame = 0;

    const tick = () => {
      frame = requestAnimationFrame(tick);
      const video = videoRef.current;
      // The playhead follows sound and picture, not the request for them.
      //
      // `play()` returns before the decoders are warm, and a freshly written
      // mix on its first play is exactly when that gap is widest: the element
      // reports a position that creeps forward, then stops dead while it
      // buffers, then jumps on. Drawn, that is a scrubber that starts, stalls
      // and restarts - which reads as a stutter even though the finished
      // playback is smooth. Below `HAVE_FUTURE_DATA` the element cannot
      // actually advance, so the playhead simply waits where it is.
      // Nothing is published during a hold: the element is already running
      // out of sight, and the whole point is that the scrubber does not move
      // until there is sound to move with.
      if (
        video &&
        !isHeldRef.current &&
        !video.seeking &&
        video.readyState >= HAVE_FUTURE_DATA
      )
        publish(video.currentTime);
    };

    frame = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(frame);
    };
  }, [isPlaying, publish]);

  /**
   * Takes the frame on screen before the element is pointed anywhere else.
   *
   * Changing `src` tears down what is loaded, and the element paints nothing
   * until the next file has decoded a frame - which is the flash a person sees
   * when they toggle a track. So the swap is deferred by one commit: this
   * captures what is showing now, and only then does `renderedSource` move,
   * leaving the held frame in front of an element that is reloading behind it.
   */
  useEffect(() => {
    if (source === renderedSource) return;

    // Only a replacement is worth covering. The first file arriving, or the
    // artifact going away entirely, has no frame to hold on to.
    if (renderedSource && source) {
      // Marked before anything else, because the remix effect below runs later
      // in this same flush and reads it to decide whether the regeneration is
      // over or merely moving on to its next stage.
      isSwapPendingRef.current = true;
      freeze("swap");
    }
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setRenderedSource(source);
  }, [freeze, renderedSource, source]);

  /**
   * Makes "remix" span the whole regeneration rather than just the build.
   *
   * Declared *after* the swap-arm effect on purpose: effects run in
   * declaration order within a flush, and `setMixUrl(url)` and
   * `setIsRemixing(false)` arrive in one commit, so the swap has already
   * marked itself pending by the time this sees `isRemixing` go false. Without
   * that ordering the hold would be released here and re-taken on the next
   * commit, which is one painted frame of loader-off - the flash.
   *
   * There is deliberately no release for "remix" in here. It is let go in
   * `settle` below, together with "swap", so the whole operation has exactly
   * one end.
   */
  useEffect(() => {
    isRemixingRef.current = isRemixing;
    if (isRemixing) freeze("remix");
    else if (!isSwapPendingRef.current) thaw("remix");
  }, [freeze, isRemixing, thaw]);

  /**
   * Carries the position and the play state across a change of file, and puts
   * the picture back only once the new one is showing the same frame.
   *
   * Pointing the element at a different `src` reloads it from the beginning
   * and pauses it, which for the user is a toggle throwing away where they
   * were. The position is whatever was last published, restored once the new
   * file knows how long it is.
   */
  useEffect(() => {
    const video = videoRef.current;
    if (!video || !renderedSource) return;

    const seconds = positionRef.current;
    const shouldResume = isPlayingRef.current;
    logPreview("source.changed", { seconds, shouldResume });
    // A different file is a different element as far as the renderer is
    // concerned, so its first start is the slow one again. Nothing is
    // published: `video` is the replacement and is sitting at 0.
    endHold({ publish: false });
    hasPlayedRef.current = false;

    let settled = false;
    /**
     * The single end of the whole operation: the build, the swap and the wait
     * for the replacement to show a frame all finish here, at once.
     */
    const settle = (reason: string) => () => {
      if (settled) return;
      settled = true;
      logPreview("source.settled", { reason, seconds: video.currentTime });
      isSwapPendingRef.current = false;
      thaw("swap");
      // Unless another build is already under way, in which case the hold
      // belongs to that one and its own settle will let it go.
      if (!isRemixingRef.current) thaw("remix");
    };
    // Whatever happens, the picture is handed back. A file that will not load
    // must not leave the last frame of the previous one standing in for it.
    const rescue = window.setTimeout(settle("timeout"), SWAP_TIMEOUT_MS);
    const failed = settle("error");
    const ready = settle("ready");
    /** Which event the reveal is waiting on, so it can be taken back off. */
    let readyEvent: "loadeddata" | "seeked" | null = null;
    video.addEventListener("error", failed, { once: true });

    const restore = () => {
      const needsSeek =
        seconds > 0 && Math.abs(video.currentTime - seconds) > 0.001;
      if (needsSeek) video.currentTime = seconds;
      // Revealed on the frame, not on the metadata: `loadedmetadata` only says
      // how long the file is, and letting go there shows frame zero of it for
      // an instant before the seek lands.
      readyEvent = needsSeek ? "seeked" : "loadeddata";
      video.addEventListener(readyEvent, ready, { once: true });

      if (!shouldResume) return;
      // Held exactly as a press is: a swap that resumed straight away would
      // show the same silent third of a second the hold exists to remove.
      startHeld(video);
      video.play().catch((cause: unknown) => {
        const error = cause as Error;
        logPreview("source.resume.rejected", {
          message: error.message,
          name: error.name,
        });
        endHold();
        isPlayingRef.current = false;
        setIsPlaying(false);
      });
    };

    video.addEventListener("loadedmetadata", restore, { once: true });
    return () => {
      clearTimeout(rescue);
      video.removeEventListener("loadedmetadata", restore);
      video.removeEventListener("error", failed);
      if (readyEvent) video.removeEventListener(readyEvent, ready);
      // Deliberately not settled here. This runs on the way *into* the next
      // file, one commit after that file's frame was taken - settling would
      // hand the picture back to an element that has just been told to reload,
      // which is the flash this all exists to prevent. The run that replaces
      // this one settles it instead, and the timeout covers the case where
      // that run never gets a frame.
    };
  }, [endHold, renderedSource, startHeld, thaw]);

  /**
   * A trace of what the element is doing, and nothing more.
   *
   * These used to drive the loader directly, and that was a third of the
   * flashing: a freshly written mp4 fires `waiting` and `playing` in bursts
   * while it first buffers, so a single toggle produced several on/off pairs
   * of its own on top of the two the swap already had. Buffering is not an
   * event worth interrupting the picture for. If a genuine stall ever needs
   * saying it has to be its own indicator, debounced by a few hundred
   * milliseconds and suppressed while the preview is held - not this.
   *
   * Reattached with the source because a swap replaces what is loaded, not the
   * element. Each of these fires on a change of state rather than per frame,
   * so the trace stays readable.
   */
  useEffect(() => {
    const video = videoRef.current;
    if (!video || !source) return;

    const announce = (event: string) => () => {
      logPreview(`media.${event}`, { media: describeMedia(video) });
    };
    const canPlayThrough = announce("canplaythrough");
    const rolling = announce("playing");
    const stalled = announce("stalled");
    const waiting = announce("waiting");

    video.addEventListener("canplaythrough", canPlayThrough);
    video.addEventListener("playing", rolling);
    video.addEventListener("stalled", stalled);
    video.addEventListener("waiting", waiting);

    return () => {
      video.removeEventListener("canplaythrough", canPlayThrough);
      video.removeEventListener("playing", rolling);
      video.removeEventListener("stalled", stalled);
      video.removeEventListener("waiting", waiting);
    };
  }, [source]);

  const seek = (seconds: number) => {
    // A scrub is a new decision, so the position a held start was going to
    // return to is no longer the one that was asked for.
    endHold();
    const target = clamp(seconds, 0, totalSeconds);
    if (videoRef.current) videoRef.current.currentTime = target;
    publish(target);
  };

  const pause = () => {
    const video = videoRef.current;
    endHold();
    video?.pause();
    // The loop stops with the playback, so the final position is pushed here
    // rather than left a frame short of wherever it actually stopped.
    if (video) publish(video.currentTime);
    isPlayingRef.current = false;
    setIsPlaying(false);
  };

  const play = () => {
    const video = videoRef.current;
    if (!video) return;
    if (
      video.ended ||
      video.currentTime >= totalSeconds - END_TOLERANCE_SECONDS
    )
      seek(0);
    setPlaybackError(null);
    logPreview("play.requested", { media: describeMedia(video) });

    // The frame under the press, held over the element for as long as the
    // audio renderer takes to come up. Behind it the element really is
    // playing, silently, and running on past the position that was pressed.
    startHeld(video);

    // Started from inside the press itself, and deliberately not deferred to
    // `canplaythrough`. WKWebView grants an element permission to make sound
    // only for a `play()` it can still see as part of the gesture that asked
    // for it; moved into a later task - even behind an already-resolved
    // promise - the call is refused. So the request goes out now and it is the
    // picture, not the playback, that waits for the renderer.
    video.play().catch((cause: unknown) => {
      const error = cause as Error;
      logPreview("play.rejected", { message: error.message, name: error.name });
      setPlaybackError(`Could not play the recording: ${error.name}`);
      endHold();
      isPlayingRef.current = false;
      setIsPlaying(false);
    });

    isPlayingRef.current = true;
    setIsPlaying(true);
  };

  return (
    <div className="flex flex-col gap-3">
      {/* Rebuilding a mix and swapping it in are one operation as far as
          anybody watching is concerned, so they share one hold and the loader
          goes on once and off once. It sits over the frame that is already on
          screen, so nothing is ever taken away to say something is being
          prepared. */}
      <div>
        {showsVideo ? (
          <VideoPreviewViewport
            artifactId={artifactId}
            frozenFrameRef={frozenFrameRef}
            isBusy={isBusy}
            // The one element there is carries the recording's sound.
            isMuted={false}
            onEnded={pause}
            onError={() => {
              setHasFailed(true);
            }}
            onLoadedMetadata={(event) => {
              const video = event.currentTarget;
              const seconds = Number.isFinite(video.duration)
                ? video.duration
                : 0;
              const discoveredDuration = Math.round(seconds * 1000);
              setKnownDurationMs(durationMs || discoveredDuration);
              onMetadata?.({
                durationMs: discoveredDuration,
                height: video.videoHeight,
                width: video.videoWidth,
              });
            }}
            posterUrl={posterUrl}
            videoRef={videoRef}
            videoUrl={renderedSource}
          />
        ) : posterUrl ? (
          <PreviewViewport
            alt="Recording preview"
            artifactId={artifactId}
            naturalHeight={0}
            naturalWidth={0}
            previewUrl={posterUrl}
          />
        ) : null}
      </div>

      {showsVideo ? (
        <div className="flex items-center gap-2">
          <div className="flex w-36 shrink-0 items-center gap-2">
            <ToggleButton
              aria-label={isPlaying ? "Pause preview" : "Play preview"}
              className="size-6 shrink-0"
              isSelected={isPlaying}
              off={<Play className="fill-current" size={16} />}
              onChange={(selected) => {
                if (selected) play();
                else pause();
              }}
              showFocus={false}
              size="sm"
              variant="ghost"
            >
              <Pause className="fill-current" size={16} />
            </ToggleButton>
            <span className="min-w-0 text-xxs text-muted tabular-nums">
              <ElapsedTime playhead={playhead} /> /{" "}
              {formatDuration(knownDurationMs)}
            </span>
          </div>
          <Timeline
            onSeek={(ratio) => {
              seek(ratio * totalSeconds);
            }}
            playhead={playhead}
          />
        </div>
      ) : null}

      {isPreparingAudio ? (
        <div className="flex h-14 items-center justify-center gap-3 text-xs text-muted">
          <CircularProgressBar
            aria-label="Preparing audio preview"
            isIndeterminate
            size={24}
            strokeWidth={8}
          />
          Preparing audio tracks
        </div>
      ) : null}

      {audioError ? (
        <p className="m-0 text-xs text-error">{audioError}</p>
      ) : null}

      {playbackError ? (
        <p className="m-0 text-xs text-error">{playbackError}</p>
      ) : null}

      {hasTracks ? (
        <div className="flex flex-col gap-2">
          {audioTracks.map((track) => {
            const enabled = enabledTracks.has(track.streamIndex);
            return (
              <div className="flex items-center gap-2" key={track.streamIndex}>
                <Button
                  aria-label={`${enabled ? "Exclude" : "Include"} ${track.label}`}
                  className="group w-36 justify-start"
                  onPress={() => {
                    setEnabledTracks((current) => {
                      const next = new Set(current);
                      if (next.has(track.streamIndex))
                        next.delete(track.streamIndex);
                      else next.add(track.streamIndex);
                      return next;
                    });
                  }}
                  showFocus={false}
                  size="sm"
                  variant={enabled ? "soft" : "ghost"}
                >
                  {track.kind === "microphone" ? (
                    <Mic size={15} />
                  ) : (
                    <Volume2 size={15} />
                  )}
                  <span className="min-w-0 grow truncate text-left">
                    {track.label}
                  </span>
                  {enabled ? (
                    <Check className="text-success" size={14} />
                  ) : null}
                </Button>
                <Waveform
                  enabled={enabled}
                  onSeek={(ratio) => {
                    seek(ratio * totalSeconds);
                  }}
                  playhead={playhead}
                  track={track}
                />
              </div>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}
