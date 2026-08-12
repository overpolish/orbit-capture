// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  MouseEvent,
  PointerEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { cn } from "../../lib/styling";

import {
  cancelTextRecognition,
  captureTextRegion,
  copyRecognizedText,
  recognizeCapturedText,
  CapturedTextRegion,
  TextRecognitionResult,
} from "./api";
import {
  TextRecognitionActions,
  TextRecognitionCloseAction,
} from "./text-recognition-actions";
import {
  orderedRange,
  Point,
  ScreenSelection,
  selectedText,
  selectionRects,
  selectionFrom,
  TextPosition,
  TextRange,
  textPositionAt,
  withoutLineBreaks,
} from "./text-selection";

const monitorId = Number(
  new URLSearchParams(window.location.search).get("monitorId") ?? 0,
);
const isMac = navigator.userAgent.includes("Mac");

const imageUrl = (bytes: number[]) =>
  URL.createObjectURL(
    new Blob([Uint8Array.from(bytes)], { type: "image/png" }),
  );

export function TextRecognitionWindow() {
  const [start, setStart] = useState<Point>();
  const [selection, setSelection] = useState<ScreenSelection>();
  const [status, setStatus] = useState<"selecting" | "loading" | "ready">(
    "selecting",
  );
  const [result, setResult] = useState<TextRecognitionResult>();
  const [capture, setCapture] = useState<CapturedTextRegion>();
  const [error, setError] = useState<string>();
  const [textAnchor, setTextAnchor] = useState<TextPosition>();
  const [textFocus, setTextFocus] = useState<TextPosition>();
  const [textRanges, setTextRanges] = useState<readonly TextRange[]>([]);
  const [selectingText, setSelectingText] = useState(false);
  const textAnchorRef = useRef<TextPosition | undefined>(undefined);
  const textFocusRef = useRef<TextPosition | undefined>(undefined);
  const selectionRef = useRef<HTMLDivElement>(null);
  const frozenUrl = useMemo(
    () => (capture ? imageUrl(capture.imagePng) : undefined),
    [capture],
  );

  useEffect(
    () => () => {
      if (frozenUrl) URL.revokeObjectURL(frozenUrl);
    },
    [frozenUrl],
  );

  const close = useCallback(() => {
    void cancelTextRecognition();
  }, []);

  const copyAndClose = useCallback((text: string) => {
    if (!text) return;
    void copyRecognizedText(text).then(() => {
      void cancelTextRecognition();
    });
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
      if (!(event.metaKey || event.ctrlKey) || !result) return;

      if (event.key.toLowerCase() === "a" && result.lines.length > 0) {
        event.preventDefault();
        const lastLine = result.lines.length - 1;
        setTextAnchor(undefined);
        setTextFocus(undefined);
        textAnchorRef.current = undefined;
        textFocusRef.current = undefined;
        setTextRanges([
          {
            end: { line: lastLine, offset: result.lines[lastLine].text.length },
            start: { line: 0, offset: 0 },
          },
        ]);
      }
      if (event.key.toLowerCase() === "c") {
        const current =
          textAnchor && textFocus ? [orderedRange(textAnchor, textFocus)] : [];
        const text = selectedText(result, [...textRanges, ...current]);
        if (!text) return;
        event.preventDefault();
        copyAndClose(text);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [close, copyAndClose, result, textAnchor, textFocus, textRanges]);

  const positionFromPointer = (
    event: PointerEvent<HTMLElement> | MouseEvent<HTMLElement>,
  ) => {
    if (!result || !selection || result.lines.length === 0) return;
    return textPositionAt(result, {
      x: (event.clientX - selection.x) / selection.width,
      y: (event.clientY - selection.y) / selection.height,
    });
  };

  const begin = (event: PointerEvent<HTMLElement>) => {
    if (status !== "selecting") return;
    event.currentTarget.setPointerCapture(event.pointerId);
    const point = { x: event.clientX, y: event.clientY };
    setError(undefined);
    setStart(point);
    setSelection({ height: 0, width: 0, ...point });
  };

  const move = (event: PointerEvent<HTMLElement>) => {
    if (!start || status !== "selecting") return;
    setSelection(selectionFrom(start, { x: event.clientX, y: event.clientY }));
  };

  const finish = (event: PointerEvent<HTMLElement>) => {
    if (!start || status !== "selecting") return;
    const next = selectionFrom(start, {
      x: event.clientX,
      y: event.clientY,
    });
    setStart(undefined);
    setSelection(next);
    if (next.width < 2 || next.height < 2) {
      setSelection(undefined);
      return;
    }
    setStatus("loading");
    captureTextRegion(monitorId, {
      position: { x: next.x, y: next.y },
      size: { height: next.height, width: next.width },
    })
      .then((captured) => {
        setCapture(captured);
        return recognizeCapturedText();
      })
      .then((recognized) => {
        setResult(recognized);
        setStatus("ready");
      })
      .catch((reason: unknown) => {
        setError(String(reason));
        setStatus("selecting");
      });
  };

  const reset = () => {
    setCapture(undefined);
    setError(undefined);
    setResult(undefined);
    setSelection(undefined);
    setTextAnchor(undefined);
    setTextFocus(undefined);
    textAnchorRef.current = undefined;
    textFocusRef.current = undefined;
    setTextRanges([]);
    setStatus("selecting");
  };

  return (
    <main
      className={cn(
        "relative h-screen w-screen overflow-hidden select-none",
        status === "selecting" && "cursor-crosshair",
      )}
      onPointerCancel={finish}
      onPointerDown={begin}
      onPointerMove={move}
      onPointerUp={finish}
    >
      {status === "selecting" && (
        <div className="pointer-events-none absolute inset-0 bg-black/20" />
      )}

      {selection && (
        <div
          className={cn(
            "absolute overflow-hidden rounded-sm before:pointer-events-none before:absolute before:inset-0 before:z-10 before:rounded-[inherit] before:border-2 before:border-dashed before:border-white before:content-['']",
            status === "loading" &&
              "ocr-shine-border before:border-transparent",
            status === "ready" && "before:border-info",
          )}
          ref={selectionRef}
          style={{
            height: selection.height,
            left: selection.x,
            top: selection.y,
            width: selection.width,
          }}
        >
          {(status === "loading" || status === "ready") && frozenUrl && (
            <>
              <img
                alt="Selected screen region"
                className="absolute inset-0 size-full object-fill"
                draggable={false}
                src={frozenUrl}
              />
              {status === "ready" && result && (
                <div
                  aria-label="Recognized text"
                  className="absolute inset-0 cursor-text touch-none"
                  onDoubleClick={(event) => {
                    event.stopPropagation();
                    const position = positionFromPointer(event);
                    if (!position) return;
                    const line = result.lines[position.line];
                    const next = {
                      end: { line: position.line, offset: line.text.length },
                      start: { line: position.line, offset: 0 },
                    };
                    setTextRanges((current) =>
                      event.metaKey || event.ctrlKey
                        ? [...current, next]
                        : [next],
                    );
                    setTextAnchor(undefined);
                    setTextFocus(undefined);
                    textAnchorRef.current = undefined;
                    textFocusRef.current = undefined;
                  }}
                  onPointerDown={(event) => {
                    event.stopPropagation();
                    const position = positionFromPointer(event);
                    if (!position) return;
                    event.currentTarget.setPointerCapture(event.pointerId);
                    if (!(event.metaKey || event.ctrlKey)) setTextRanges([]);
                    textAnchorRef.current = position;
                    textFocusRef.current = position;
                    setTextAnchor(position);
                    setTextFocus(position);
                    setSelectingText(true);
                  }}
                  onPointerMove={(event) => {
                    if (!selectingText) return;
                    const position = positionFromPointer(event);
                    if (position) {
                      textFocusRef.current = position;
                      setTextFocus(position);
                    }
                  }}
                  onPointerUp={(event) => {
                    event.stopPropagation();
                    const anchor = textAnchorRef.current;
                    const focus =
                      positionFromPointer(event) ?? textFocusRef.current;
                    if (anchor && focus) {
                      const range = orderedRange(anchor, focus);
                      setTextRanges((current) => [...current, range]);
                    }
                    textAnchorRef.current = undefined;
                    textFocusRef.current = undefined;
                    setTextAnchor(undefined);
                    setTextFocus(undefined);
                    setSelectingText(false);
                  }}
                >
                  {result.lines.map((line, index) => (
                    <span
                      className="pointer-events-none absolute rounded-[2px] bg-info/20 outline outline-1 outline-info/65"
                      key={`detected-${line.text}-${index.toString()}`}
                      style={{
                        height: `${(line.bounds.height * 100).toString()}%`,
                        left: `${(line.bounds.x * 100).toString()}%`,
                        top: `${(line.bounds.y * 100).toString()}%`,
                        width: `${(line.bounds.width * 100).toString()}%`,
                      }}
                    />
                  ))}
                  {selectionRects(result, [
                    ...textRanges,
                    ...(textAnchor && textFocus
                      ? [orderedRange(textAnchor, textFocus)]
                      : []),
                  ]).map((rect, index) => (
                    <span
                      className="pointer-events-none absolute rounded-[2px] bg-info/45 outline outline-1 outline-info/80"
                      key={`${rect.x.toString()}-${rect.y.toString()}-${index.toString()}`}
                      style={{
                        height: `${(rect.height * 100).toString()}%`,
                        left: `${(rect.x * 100).toString()}%`,
                        top: `${(rect.y * 100).toString()}%`,
                        width: `${(rect.width * 100).toString()}%`,
                      }}
                    />
                  ))}
                </div>
              )}
            </>
          )}
        </div>
      )}

      {status === "loading" && selection && (
        <div
          className="pointer-events-none absolute grid place-items-center"
          style={{
            height: selection.height,
            left: selection.x,
            top: selection.y,
            width: selection.width,
          }}
        >
          <span className="rounded-md border border-muted/20 bg-content/90 px-3 py-1.5 text-sm text-content-fg shadow-md backdrop-blur-md">
            Finding text…
          </span>
        </div>
      )}

      {status === "ready" && result && selection && (
        <div
          className="absolute flex max-w-[calc(100vw-16px)] items-center gap-1 rounded-md border border-muted/25 bg-content p-2 shadow-md"
          style={{
            left: Math.min(selection.x, window.innerWidth - 220),
            top:
              selection.y + selection.height + 8 < window.innerHeight - 44
                ? selection.y + selection.height + 8
                : Math.max(8, selection.y - 44),
          }}
        >
          <TextRecognitionActions
            onClose={close}
            onCopyAll={() => {
              copyAndClose(result.text);
            }}
            onCopyAsParagraph={() => {
              const current =
                textAnchor && textFocus
                  ? [orderedRange(textAnchor, textFocus)]
                  : [];
              const selectionText = selectedText(result, [
                ...textRanges,
                ...current,
              ]);
              copyAndClose(withoutLineBreaks(selectionText || result.text));
            }}
            onReset={reset}
          />
        </div>
      )}

      {status !== "ready" && (
        <TextRecognitionCloseAction isMac={isMac} onClose={close} />
      )}

      {error && (
        <div className="absolute bottom-4 left-1/2 -translate-x-1/2 rounded-md border border-error/30 bg-content/94 px-3 py-2 text-sm text-error shadow-md">
          {error}
        </div>
      )}
    </main>
  );
}
