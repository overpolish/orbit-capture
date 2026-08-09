// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ImageIcon } from "lucide-react";
import {
  RefObject,
  SyntheticEvent,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";

import { CircularProgressBar } from "../../../components/base/circular-progress-bar/circular-progress-bar";
import { Overlay } from "../../../components/base/overlay/overlay";
import { describeMedia, logPreview } from "../diagnostics";

/** Zoom is relative to fit, so 1 is "the whole capture on screen". */
const FIT = 1;
/** Small captures can still be magnified even when that passes native pixels. */
const MIN_MAX_ZOOM = 4;
/** Pinch deltas are small and arrive continuously, so the rate stays gentle. */
const PINCH_ZOOM_RATE = 0.01;
/**
 * A mouse-wheel notch carries a far larger delta than a pinch step (~100 vs a
 * handful), so it needs a much smaller rate to land near one comfortable zoom
 * step per notch instead of jumping across the whole range.
 */
const WHEEL_ZOOM_RATE = 0.0015;
const RESET_TRANSITION = "transform 160ms ease-out";

/**
 * Only the Mac trackpad distinguishes a pinch (wheel event with `ctrlKey`) from
 * a two-finger scroll, so only there does a plain scroll pan. Everywhere else a
 * plain scroll wheel zooms directly, and panning is click-and-drag.
 */
const isMac = navigator.userAgent.includes("Mac");

/**
 * Holds the frame that is on screen over the video, so playback can be started
 * out of sight.
 *
 * Owned here because the frame has to be drawn at the same size, position and
 * zoom as the video under it, and this is the component that knows what those
 * are. See `AUDIO_WARMUP_MS` in `scrub-preview.tsx` for why a start has to be
 * hidden at all.
 */
export type FrozenFrame = {
  capture: () => void;
  clear: () => void;
};

type Transform = { x: number; y: number; zoom: number };
type Geometry = {
  boxHeight: number;
  boxWidth: number;
  fitScale: number;
  naturalHeight: number;
  naturalWidth: number;
};

type PreviewViewportProps = {
  alt: string;
  artifactId: number;
  naturalHeight: number;
  naturalWidth: number;
  onNeedFullResolution?: () => void;
  previewUrl?: string | null;
};

type VideoPreviewViewportProps = {
  artifactId: number;
  videoRef: RefObject<HTMLVideoElement | null>;
  frozenFrameRef?: RefObject<FrozenFrame | null>;
  /** Covers the picture while a different file is being prepared or loaded. */
  isBusy?: boolean;
  /** Silent only when something else is carrying the recording's audio. */
  isMuted?: boolean;
  onEnded?: () => void;
  onError?: () => void;
  onLoadedMetadata?: (event: SyntheticEvent<HTMLVideoElement>) => void;
  posterUrl?: string | null;
  videoUrl?: string | null;
};

type MediaPreviewViewportProps = {
  alt: string;
  artifactId: number;
  mediaKind: "image" | "video";
  naturalHeight: number;
  naturalWidth: number;
  frozenFrameRef?: RefObject<FrozenFrame | null>;
  isBusy?: boolean;
  isMuted?: boolean;
  onEnded?: () => void;
  onError?: () => void;
  onLoadedMetadata?: (event: SyntheticEvent<HTMLVideoElement>) => void;
  onNeedFullResolution?: () => void;
  posterUrl?: string | null;
  previewUrl?: string | null;
  videoRef?: RefObject<HTMLVideoElement | null>;
};

const clamp = (value: number, min: number, max: number) =>
  Math.min(Math.max(value, min), max);

function MediaPreviewViewport({
  alt,
  artifactId,
  frozenFrameRef,
  isBusy = false,
  isMuted = true,
  mediaKind,
  naturalHeight,
  naturalWidth,
  onEnded,
  onError,
  onLoadedMetadata,
  onNeedFullResolution,
  posterUrl,
  previewUrl,
  videoRef,
}: MediaPreviewViewportProps) {
  const boxRef = useRef<HTMLDivElement>(null);
  const mediaRef = useRef<HTMLImageElement | HTMLVideoElement>(null);
  const frozenRef = useRef<HTMLCanvasElement>(null);
  // The live transform lives in a ref and is written straight to the element:
  // routing every wheel event through React state made each one compute from a
  // stale value, which is what made the gesture judder.
  const transformRef = useRef<Transform>({ x: 0, y: 0, zoom: FIT });
  const frameRef = useRef<number | undefined>(undefined);
  const geometryRef = useRef<Geometry>({
    boxHeight: 0,
    boxWidth: 0,
    fitScale: 1,
    naturalHeight,
    naturalWidth,
  });
  const panRef = useRef<{ pointer: Transform; start: Transform } | null>(null);
  /** The last fit that was worth reporting, so identical ones stay quiet. */
  const fitSignatureRef = useRef("");
  const requestedFullRef = useRef(false);
  /**
   * The current callback, so the wheel listener can be bound once.
   *
   * Its caller passes an inline arrow, so depending on the prop directly would
   * be a fresh `addEventListener`/`removeEventListener` pair on every render -
   * which is what the listener effect below used to do, having no dependency
   * array at all.
   */
  const onNeedFullResolutionRef = useRef(onNeedFullResolution);
  /** The artifact whose bitmap the current `src` belongs to. */
  const sourceArtifactRef = useRef(artifactId);
  const [zoomPercent, setZoomPercent] = useState(100);

  const isZoomed = zoomPercent > 100;

  /** Writes the current transform to the element. Geometry must already be right. */
  const applyTransform = (reveal: boolean) => {
    const media = mediaRef.current;
    if (!media) return;
    const { x, y, zoom } = transformRef.current;
    const scale = geometryRef.current.fitScale * zoom;

    media.style.transform = `translate(${x.toString()}px, ${y.toString()}px) scale(${scale.toString()})`;
    // The shadow is drawn in the image's own coordinates, which the transform
    // then scales, so it is divided back out to stay a constant size on screen.
    const inverse = scale > 0 ? 1 / scale : 1;
    media.style.boxShadow = `0 ${(2 * inverse).toString()}px ${(12 * inverse).toString()}px rgb(0 0 0 / 0.28)`;
    if (reveal) media.style.opacity = "1";

    // The held frame is taken out of the flex flow so it can sit exactly on
    // top, so it is centred here rather than by the box, and then given the
    // same transform about the same centre.
    const frozen = frozenRef.current;
    if (frozen) {
      // Its own pixel size, not the geometry's: a held frame outlives the file
      // it came from, and must not be restretched when the next one measures.
      frozen.style.width = `${frozen.width.toString()}px`;
      frozen.style.height = `${frozen.height.toString()}px`;
      frozen.style.transform = `translate(-50%, -50%) translate(${x.toString()}px, ${y.toString()}px) scale(${scale.toString()})`;
      frozen.style.boxShadow = media.style.boxShadow;
    }
  };

  /**
   * Measuring and applying are one operation, gated on the bitmap actually
   * being decoded.
   *
   * They used to be separate, and the element's layout size was rendered from
   * `geometryRef` - a ref that `measure` mutated outside the render cycle. A
   * replacement capture therefore painted a 640px thumbnail laid out at the
   * previous capture's 3456px with a fit scale computed for 640, which is
   * exactly the "slightly zoomed" state. The image now carries no width or
   * height attributes at all, so its layout size *is* its natural size and the
   * two cannot disagree.
   */
  const measureAndApply = (trigger: string) => {
    const box = boxRef.current;
    const media = mediaRef.current;
    if (!box || !media) {
      logPreview("fit.skipped", { reason: "no element", trigger });
      return;
    }
    const isVideo = media instanceof HTMLVideoElement;
    const width = isVideo ? media.videoWidth : media.naturalWidth;
    const height = isVideo ? media.videoHeight : media.naturalHeight;
    const boxWidth = box.clientWidth;
    const boxHeight = box.clientHeight;

    // Before decode/metadata, intrinsic dimensions are 0 and any fit scale
    // would be a guess. The box can also be measured before it has been laid
    // out, which would make every scale derived from it far too small.
    if (
      (!isVideo && !media.complete) ||
      width === 0 ||
      height === 0 ||
      boxWidth === 0 ||
      boxHeight === 0
    ) {
      logPreview("fit.deferred", {
        boxHeight,
        boxWidth,
        media: describeMedia(media),
        trigger,
      });
      return;
    }

    const fitScale = Math.min(1, boxWidth / width, boxHeight / height);
    geometryRef.current = {
      boxHeight,
      boxWidth,
      // Never below 1:1 - a capture smaller than the box sits at its own size.
      fitScale,
      naturalHeight: height,
      naturalWidth: width,
    };
    // The scale multiplies the element's laid-out size, so the layout has to
    // agree with the intrinsic size or the picture comes out at the wrong
    // scale entirely. An image's layout size is its natural size, but a video
    // with a poster is laid out at the poster's size until frames arrive -
    // so it is told, explicitly, how big it is.
    if (isVideo) {
      media.style.width = `${width.toString()}px`;
      media.style.height = `${height.toString()}px`;
    }

    // Only when the fit actually changed. This runs after every render, and a
    // video that reports a new `currentTime` on each frame would otherwise
    // bury every other line in the trace under sixty identical fits a second.
    const signature = `${boxWidth.toString()}x${boxHeight.toString()}:${width.toString()}x${height.toString()}@${fitScale.toString()}`;
    if (fitSignatureRef.current !== signature) {
      fitSignatureRef.current = signature;
      logPreview("fit.applied", {
        boxHeight,
        boxWidth,
        fitScale,
        media: describeMedia(media),
        trigger,
      });
    }
    applyTransform(true);
  };

  // Published once, on mount, and taken back only on unmount. Both halves read
  // nothing but refs, so the closure captured here stays correct for the life
  // of the component - and the parent's freeze can never arrive to find the
  // ref momentarily null, which is what a dependency-less effect risked:
  // re-running it on every render meant nulling and republishing the ref
  // between every commit.
  useEffect(() => {
    if (!frozenFrameRef) return;

    frozenFrameRef.current = {
      capture: () => {
        const media = mediaRef.current;
        const frozen = frozenRef.current;
        if (!frozen || !(media instanceof HTMLVideoElement)) return;
        const { videoHeight, videoWidth } = media;
        if (videoWidth === 0 || videoHeight === 0) return;
        // Sized to the frame rather than to the screen: the transform above
        // scales it to match whatever zoom the video is being shown at.
        frozen.width = videoWidth;
        frozen.height = videoHeight;
        const context = frozen.getContext("2d");
        if (!context) return;
        context.drawImage(media, 0, 0, videoWidth, videoHeight);
        frozen.style.display = "block";
        applyTransform(false);
      },
      clear: () => {
        const frozen = frozenRef.current;
        if (frozen) frozen.style.display = "none";
      },
    };

    return () => {
      frozenFrameRef.current = null;
    };
    // `applyTransform` is rewritten every render but only ever reads refs, so
    // there is nothing here for it to go stale against.
  }, [frozenFrameRef]);

  const schedule = () => {
    if (frameRef.current !== undefined) return;
    frameRef.current = requestAnimationFrame(() => {
      frameRef.current = undefined;
      applyTransform(false);
      const percent = Math.round(transformRef.current.zoom * 100);
      setZoomPercent((current) => (current === percent ? current : percent));
    });
  };

  /** Zooming all the way in lands on the capture's own pixels, exactly 1:1. */
  const maxZoom = () =>
    Math.max(MIN_MAX_ZOOM, 1 / (geometryRef.current.fitScale || 1));

  /** Keeps the image inside its own box at whatever zoom it is now. */
  const contained = (next: Transform): Transform => {
    const { boxHeight, boxWidth, fitScale, naturalHeight, naturalWidth } =
      geometryRef.current;
    const scale = fitScale * next.zoom;
    const slackX = Math.max(0, (naturalWidth * scale - boxWidth) / 2);
    const slackY = Math.max(0, (naturalHeight * scale - boxHeight) / 2);

    return {
      x: clamp(next.x, -slackX, slackX),
      y: clamp(next.y, -slackY, slackY),
      zoom: next.zoom,
    };
  };

  const clearTransition = () => {
    if (mediaRef.current) mediaRef.current.style.transition = "";
  };

  const reset = () => {
    transformRef.current = { x: 0, y: 0, zoom: FIT };
    if (mediaRef.current) mediaRef.current.style.transition = RESET_TRANSITION;
    applyTransform(false);
    setZoomPercent(100);
  };

  // A new bitmap stays hidden until a transform has been applied for it, so it
  // can never be painted at its raw natural size for a frame.
  useLayoutEffect(() => {
    sourceArtifactRef.current = artifactId;
    const media = mediaRef.current;
    if (media) media.style.opacity = "0";
  }, [previewUrl, artifactId]);

  // The reveal for a new bitmap, paired with the effect above that hid it.
  // Declared after it so it runs after it, and a no-op until the bitmap is
  // decoded - which is the usual case here, with the media events and the
  // resize observer below doing the actual measuring once it is.
  //
  // It ran after *every* render before, which meant a full measure-and-apply
  // for every unrelated state change in the tree above - the loader turning on
  // and off, a zoom badge moving - none of which can change the fit.
  useLayoutEffect(() => {
    measureAndApply("render");
    // As with the observer below: this reads only refs and the DOM.
    // eslint-disable-next-line @eslint-react/exhaustive-deps
  }, [artifactId, previewUrl]);

  useEffect(() => {
    requestedFullRef.current = false;
    fitSignatureRef.current = "";
    transformRef.current = { x: 0, y: 0, zoom: FIT };
    clearTransition();
    // Syncing the badge to a new capture, which is exactly what this effect
    // exists for.
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setZoomPercent(100);
  }, [artifactId]);

  useEffect(() => {
    onNeedFullResolutionRef.current = onNeedFullResolution;
  }, [onNeedFullResolution]);

  useEffect(
    () => () => {
      if (frameRef.current !== undefined)
        cancelAnimationFrame(frameRef.current);
    },
    [],
  );

  // The fit scale is derived from the box, so it has to be recomputed whenever
  // the box changes size. The window resizes itself to its content, so this
  // happens on ordinary use rather than only when a person drags an edge.
  useEffect(() => {
    const box = boxRef.current;
    if (!box) return;

    const observer = new ResizeObserver(() => {
      measureAndApply("resize-observer");
    });
    observer.observe(box);

    return () => {
      observer.disconnect();
    };
    // `measureAndApply` only ever reads refs, so the closure captured on mount
    // stays correct for the life of the component.
    // eslint-disable-next-line @eslint-react/exhaustive-deps
  }, []);

  // Attached natively so `preventDefault` is honoured: React's wheel listener
  // is passive, and without it the window rubber-bands during a gesture.
  useEffect(() => {
    const box = boxRef.current;
    if (!box) return;

    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      clearTransition();
      const current = transformRef.current;

      // On a Mac trackpad a pinch arrives as a wheel event with ctrlKey set:
      // only that zooms, and a plain two-finger scroll pans, the way Preview
      // does it. Everywhere else the scroll wheel zooms directly (Ctrl+wheel
      // still zooms too). The rate follows the gesture, not the modifier: a
      // Mac pinch takes the gentle pinch rate, any wheel takes the wheel rate.
      const isPinch = isMac && event.ctrlKey;
      if (!isMac || event.ctrlKey) {
        const rate = isPinch ? PINCH_ZOOM_RATE : WHEEL_ZOOM_RATE;
        const next = clamp(
          current.zoom * Math.exp(-event.deltaY * rate),
          FIT,
          maxZoom(),
        );
        if (next === current.zoom) return;

        if (next > FIT && !requestedFullRef.current) {
          requestedFullRef.current = true;
          onNeedFullResolutionRef.current?.();
        }

        const bounds = box.getBoundingClientRect();
        const pointerX = event.clientX - (bounds.left + bounds.width / 2);
        const pointerY = event.clientY - (bounds.top + bounds.height / 2);
        const ratio = next / current.zoom;
        transformRef.current = contained({
          x: pointerX - (pointerX - current.x) * ratio,
          y: pointerY - (pointerY - current.y) * ratio,
          zoom: next,
        });
      } else {
        // Nothing to pan while the whole capture is already on screen.
        if (current.zoom <= FIT) return;
        transformRef.current = contained({
          x: current.x - event.deltaX,
          y: current.y - event.deltaY,
          zoom: current.zoom,
        });
      }

      schedule();
    };

    box.addEventListener("wheel", onWheel, { passive: false });

    return () => {
      box.removeEventListener("wheel", onWheel);
    };
    // Bound once: everything the handler reads is a ref, including the
    // callback, so a re-bind on every render bought nothing but garbage.
    // eslint-disable-next-line @eslint-react/exhaustive-deps
  }, []);

  return (
    <div
      // `shrink-0` is load-bearing: this is a flex item inside a column, and a
      // flex item with `overflow: hidden` has an automatic minimum size of
      // zero. Left shrinkable, a window that is briefly too short squeezes the
      // box, and the fit scale - which is derived from the box - shrinks with
      // it. That is what made the preview open tiny.
      className="relative flex h-[220px] shrink-0 touch-none items-center justify-center overflow-hidden overscroll-contain rounded-md"
      onDoubleClick={reset}
      onPointerDown={(event) => {
        if (transformRef.current.zoom <= FIT) return;
        clearTransition();
        event.currentTarget.setPointerCapture(event.pointerId);
        panRef.current = {
          pointer: { x: event.clientX, y: event.clientY, zoom: 1 },
          start: transformRef.current,
        };
      }}
      onPointerMove={(event) => {
        const pan = panRef.current;
        if (!pan) return;
        transformRef.current = contained({
          x: pan.start.x + (event.clientX - pan.pointer.x),
          y: pan.start.y + (event.clientY - pan.pointer.y),
          zoom: pan.start.zoom,
        });
        schedule();
      }}
      onPointerUp={(event) => {
        panRef.current = null;
        event.currentTarget.releasePointerCapture(event.pointerId);
      }}
      ref={boxRef}
      style={{ cursor: isZoomed ? "grab" : "default" }}
    >
      {previewUrl && mediaKind === "image" ? (
        <img
          alt={alt}
          // No width or height: the intrinsic size *is* the natural size, which
          // is what keeps the layout and the fit scale in agreement.
          className="max-w-none shrink-0 select-none"
          draggable={false}
          onLoad={() => {
            // Drops a bitmap that finished arriving after the capture moved on.
            if (sourceArtifactRef.current !== artifactId) return;
            measureAndApply("image-load");
          }}
          ref={(element) => {
            mediaRef.current = element;
          }}
          src={previewUrl}
          style={{ transformOrigin: "center center" }}
        />
      ) : previewUrl && mediaKind === "video" ? (
        <video
          className="max-w-none shrink-0 select-none"
          muted={isMuted}
          onCanPlay={() => {
            measureAndApply("canplay");
          }}
          onEnded={onEnded}
          onError={(event) => {
            logPreview("video.error", {
              media: describeMedia(event.currentTarget),
            });
            onError?.();
          }}
          onLoadedData={() => {
            measureAndApply("loadeddata");
          }}
          onLoadedMetadata={(event) => {
            measureAndApply("loadedmetadata");
            onLoadedMetadata?.(event);
          }}
          onPlay={(event) => {
            logPreview("video.play", {
              media: describeMedia(event.currentTarget),
            });
            measureAndApply("play");
          }}
          onResize={() => {
            // Fires when the video's own dimensions change, which is the
            // moment a poster-sized layout becomes a frame-sized one.
            measureAndApply("video-resize");
          }}
          playsInline
          poster={posterUrl ?? undefined}
          preload="auto"
          ref={(element) => {
            mediaRef.current = element;
            if (videoRef) videoRef.current = element;
          }}
          src={previewUrl}
          style={{ transformOrigin: "center center" }}
        />
      ) : (
        <ImageIcon className="text-muted/50" size={40} />
      )}

      {/* Always present so a press never has to wait for it to mount. */}
      <canvas
        aria-hidden="true"
        className="pointer-events-none absolute top-1/2 left-1/2 max-w-none select-none"
        ref={frozenRef}
        style={{ display: "none", transformOrigin: "center center" }}
      />

      <Overlay blur="sm" className="rounded-md" contained isOpen={isBusy}>
        <CircularProgressBar
          aria-label="Preparing the preview"
          isIndeterminate
          size={32}
          strokeWidth={10}
        />
      </Overlay>

      {isZoomed ? (
        <span className="pointer-events-none absolute right-2 bottom-2 rounded bg-content/80 px-1.5 py-0.5 text-xxs text-muted tabular-nums">
          {zoomPercent}% double-click to fit
        </span>
      ) : null}
    </div>
  );
}

export function PreviewViewport(props: PreviewViewportProps) {
  return <MediaPreviewViewport {...props} mediaKind="image" />;
}

export function VideoPreviewViewport({
  artifactId,
  frozenFrameRef,
  isBusy,
  isMuted,
  onEnded,
  onError,
  onLoadedMetadata,
  posterUrl,
  videoRef,
  videoUrl,
}: VideoPreviewViewportProps) {
  return (
    <MediaPreviewViewport
      alt="Recording preview"
      artifactId={artifactId}
      frozenFrameRef={frozenFrameRef}
      isBusy={isBusy}
      isMuted={isMuted}
      mediaKind="video"
      naturalHeight={0}
      naturalWidth={0}
      onEnded={onEnded}
      onError={onError}
      onLoadedMetadata={onLoadedMetadata}
      posterUrl={posterUrl}
      previewUrl={videoUrl}
      videoRef={videoRef}
    />
  );
}
