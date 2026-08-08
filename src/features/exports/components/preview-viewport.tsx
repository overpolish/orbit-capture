import { ImageIcon } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";

/** Zoom is relative to fit, so 1 is "the whole capture on screen". */
const FIT = 1;
/** Small captures can still be magnified even when that passes native pixels. */
const MIN_MAX_ZOOM = 4;
/** Pinch deltas are small and arrive continuously, so the rate stays gentle. */
const ZOOM_RATE = 0.01;
const RESET_TRANSITION = "transform 160ms ease-out";

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

const clamp = (value: number, min: number, max: number) =>
  Math.min(Math.max(value, min), max);

export function PreviewViewport({
  alt,
  artifactId,
  naturalHeight,
  naturalWidth,
  onNeedFullResolution,
  previewUrl,
}: PreviewViewportProps) {
  const boxRef = useRef<HTMLDivElement>(null);
  const imageRef = useRef<HTMLImageElement>(null);
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
  const requestedFullRef = useRef(false);
  /** The artifact whose bitmap the current `src` belongs to. */
  const sourceArtifactRef = useRef(artifactId);
  const [zoomPercent, setZoomPercent] = useState(100);

  const isZoomed = zoomPercent > 100;

  /** Writes the current transform to the element. Geometry must already be right. */
  const applyTransform = (reveal: boolean) => {
    const image = imageRef.current;
    if (!image) return;
    const { x, y, zoom } = transformRef.current;
    const scale = geometryRef.current.fitScale * zoom;

    image.style.transform = `translate(${x.toString()}px, ${y.toString()}px) scale(${scale.toString()})`;
    // The shadow is drawn in the image's own coordinates, which the transform
    // then scales, so it is divided back out to stay a constant size on screen.
    const inverse = scale > 0 ? 1 / scale : 1;
    image.style.boxShadow = `0 ${(2 * inverse).toString()}px ${(12 * inverse).toString()}px rgb(0 0 0 / 0.28)`;
    if (reveal) image.style.opacity = "1";
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
  const measureAndApply = () => {
    const box = boxRef.current;
    const image = imageRef.current;
    if (!box || !image) return;
    // Before decode `naturalWidth` is 0 and any fit scale would be a guess.
    if (!image.complete || image.naturalWidth === 0) return;

    const boxWidth = box.clientWidth;
    const boxHeight = box.clientHeight;
    const width = image.naturalWidth;
    const height = image.naturalHeight;
    geometryRef.current = {
      boxHeight,
      boxWidth,
      // Never below 1:1 - a capture smaller than the box sits at its own size.
      fitScale: Math.min(1, boxWidth / width, boxHeight / height),
      naturalHeight: height,
      naturalWidth: width,
    };

    applyTransform(true);
  };

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
    if (imageRef.current) imageRef.current.style.transition = "";
  };

  const reset = () => {
    transformRef.current = { x: 0, y: 0, zoom: FIT };
    if (imageRef.current) imageRef.current.style.transition = RESET_TRANSITION;
    applyTransform(false);
    setZoomPercent(100);
  };

  // A new bitmap stays hidden until a transform has been applied for it, so it
  // can never be painted at its raw natural size for a frame.
  useLayoutEffect(() => {
    sourceArtifactRef.current = artifactId;
    const image = imageRef.current;
    if (image) image.style.opacity = "0";
  }, [previewUrl, artifactId]);

  // Runs after every render, and is a no-op until the bitmap is decoded.
  useLayoutEffect(() => {
    measureAndApply();
  });

  useEffect(() => {
    requestedFullRef.current = false;
    transformRef.current = { x: 0, y: 0, zoom: FIT };
    clearTransition();
    // Syncing the badge to a new capture, which is exactly what this effect
    // exists for.
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setZoomPercent(100);
  }, [artifactId]);

  useEffect(
    () => () => {
      if (frameRef.current !== undefined)
        cancelAnimationFrame(frameRef.current);
    },
    [],
  );

  // Attached natively so `preventDefault` is honoured: React's wheel listener
  // is passive, and without it the window rubber-bands during a gesture.
  useEffect(() => {
    const box = boxRef.current;
    if (!box) return;

    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      clearTransition();
      const current = transformRef.current;

      // A trackpad pinch arrives as a wheel event with ctrlKey set. Only that
      // zooms; a plain two-finger scroll pans, the way Preview does it.
      if (event.ctrlKey) {
        const next = clamp(
          current.zoom * Math.exp(-event.deltaY * ZOOM_RATE),
          FIT,
          maxZoom(),
        );
        if (next === current.zoom) return;

        if (next > FIT && !requestedFullRef.current) {
          requestedFullRef.current = true;
          onNeedFullResolution?.();
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
  });

  return (
    <div
      className="relative flex h-[220px] touch-none items-center justify-center overflow-hidden overscroll-contain rounded-md"
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
      {previewUrl ? (
        <img
          alt={alt}
          // No width or height: the intrinsic size *is* the natural size, which
          // is what keeps the layout and the fit scale in agreement.
          className="max-w-none shrink-0 select-none"
          draggable={false}
          onLoad={() => {
            // Drops a bitmap that finished arriving after the capture moved on.
            if (sourceArtifactRef.current !== artifactId) return;
            measureAndApply();
          }}
          ref={imageRef}
          src={previewUrl}
          style={{ transformOrigin: "center center" }}
        />
      ) : (
        <ImageIcon className="text-muted/50" size={40} />
      )}

      {isZoomed ? (
        <span className="pointer-events-none absolute right-2 bottom-2 rounded bg-content/80 px-1.5 py-0.5 text-xxs text-muted tabular-nums">
          {zoomPercent}% double-click to fit
        </span>
      ) : null}
    </div>
  );
}
