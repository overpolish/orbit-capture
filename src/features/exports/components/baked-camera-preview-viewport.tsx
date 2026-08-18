// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { AnimatePresence } from "motion/react";
import {
  MouseEvent as ReactMouseEvent,
  PointerEvent as ReactPointerEvent,
  RefObject,
  useRef,
  useState,
} from "react";

import { CropShade } from "../../../components/shared/canvas-tools/crop-shade";
import { TransformControls } from "../../../components/shared/canvas-tools/transform-controls";
import {
  cameraOverlayGeometry,
  fitCanvasToCameraOverlay,
  RADIUS_HANDLE_INSET,
  RADIUS_HANDLE_TRAVEL,
  resizeCameraOverlayCanvas,
} from "../camera-overlay-geometry";
import {
  resizeScreenshotCanvas,
  ScreenshotOutputSettings,
  screenshotOutputDimensions,
  screenshotWorkspaceItemOutput,
} from "../screenshot-output";
import { ScreenshotSnapGuide } from "../screenshot-snapping";
import { CameraOverlaySettings, RecordingPreviewPane } from "../types";
import { useExportEditGesture } from "../use-export-edit-history";
import { usePreviewCapabilities } from "../use-preview-capabilities";

import { InteractivePreviewViewport } from "./interactive-preview-viewport";
import { NativeRecordingWorkspaceViewport } from "./native-recording-workspace-viewport";
import { RecordingCanvasTool } from "./recording-crop-toggle";
import { ScreenshotCanvasControl } from "./screenshot-canvas-control";
import { ScreenshotCropMagnifier } from "./screenshot-crop-magnifier";
import { constrainedHandlePoint } from "./screenshot-layout-geometry";
import { ScreenshotPreviewLayer } from "./screenshot-preview-layer";
import { useCameraOverlayInteraction } from "./use-camera-overlay-interaction";

export function BakedCameraPreviewViewport({
  activeTrack,
  cameraCanvasRef,
  cameraPane,
  controlsVisible = true,
  interactionEnabled = true,
  isBusy,
  nativeWorkspaceEditor = false,
  onCanvasResizeDraft,
  onInteractionEnd,
  onInteractionStart,
  onOutputChange,
  onSelectTrack,
  onSettingsChange,
  onTrackContextMenu,
  onZoomChange,
  outputControlsVisible = false,
  outputSettings,
  screenCanvasRef,
  screenPane,
  settings,
  tool,
  zoomPercent,
}: {
  activeTrack: "camera" | "primary" | null;
  cameraCanvasRef: RefObject<HTMLCanvasElement | null>;
  cameraPane: RecordingPreviewPane;
  isBusy: boolean;
  outputSettings: ScreenshotOutputSettings;
  screenCanvasRef: RefObject<HTMLCanvasElement | null>;
  screenPane: RecordingPreviewPane;
  settings: CameraOverlaySettings;
  tool: RecordingCanvasTool;
  controlsVisible?: boolean;
  interactionEnabled?: boolean;
  nativeWorkspaceEditor?: boolean;
  onCanvasResizeDraft?: (settings: ScreenshotOutputSettings | null) => void;
  onInteractionEnd?: () => void;
  onInteractionStart?: () => void;
  onOutputChange?: (settings: ScreenshotOutputSettings) => void;
  onSelectTrack?: (trackId: "camera" | "primary") => void;
  onSettingsChange?: (settings: CameraOverlaySettings) => void;
  onTrackContextMenu?: (
    trackId: "camera" | "primary",
    event: ReactMouseEvent<HTMLDivElement>,
  ) => void;
  onZoomChange?: (zoomPercent: number) => void;
  outputControlsVisible?: boolean;
  zoomPercent?: number;
}) {
  const editGesture = useExportEditGesture();
  const mediaRef = useRef<HTMLDivElement | null>(null);
  const outputRef = useRef<HTMLDivElement | null>(null);
  const outputResizeRef = useRef<{
    output: { height: number; width: number };
    settings: CameraOverlaySettings;
  } | null>(null);
  // The viewport hands its resize anchoring to `renderMedia`, while the camera
  // gesture lives out here; the latest handlers are kept for it to reach.
  const mediaResizeRef = useRef<{
    onMediaResize: (bounds: {
      height: number;
      originX: number;
      originY: number;
      width: number;
    }) => void;
    onMediaResizeEnd: () => void;
    onMediaResizeStart: () => void;
  } | null>(null);
  const autoFitRef = useRef<{
    output: { height: number; width: number };
    settings: ScreenshotOutputSettings;
    used: boolean;
  } | null>(null);
  const [activeEdges, setActiveEdges] = useState<
    ("bottom" | "left" | "right" | "top")[] | null
  >(null);
  const [magnifierPoint, setMagnifierPoint] = useState({ x: 0, y: 0 });
  const [snapGuides, setSnapGuides] = useState<{
    x?: ScreenshotSnapGuide;
    y?: ScreenshotSnapGuide;
  }>({});
  const nativeSurface = usePreviewCapabilities()?.nativeRecordingPreview;
  const output = screenshotOutputDimensions(outputSettings);
  const outputWorkspace = {
    ...outputSettings,
    items: [{ id: 0, output: outputSettings }],
  };
  const outputPane = {
    ...screenPane,
    height: output.height,
    sourceHeight: output.height,
    sourceWidth: output.width,
    width: output.width,
  };
  const geometry = cameraOverlayGeometry(outputPane, cameraPane, settings);

  const endAutoFit = () => {
    if (autoFitRef.current?.used) mediaResizeRef.current?.onMediaResizeEnd();
    autoFitRef.current = null;
  };
  const { begin, interaction, naturalPoint } = useCameraOverlayInteraction({
    cameraPane,
    mediaRef,
    // Alt grows the output canvas around the camera as it leaves the frame,
    // exactly as it grows the screenshot canvas around a moved layer.
    onAutoFitCanvas: (change) => {
      if (change.autoFitStarted)
        autoFitRef.current = {
          output: change.output,
          settings: outputSettings,
          used: false,
        };
      const gesture = autoFitRef.current;
      if (!change.autoFitCanvas || !gesture) {
        endAutoFit();
        return change.settings;
      }
      const bounds = fitCanvasToCameraOverlay(
        {
          ...outputPane,
          height: gesture.output.height,
          sourceHeight: gesture.output.height,
          sourceWidth: gesture.output.width,
          width: gesture.output.width,
        },
        cameraPane,
        change.settings,
      );
      if (!gesture.used) {
        gesture.used = true;
        mediaResizeRef.current?.onMediaResizeStart();
      }
      mediaResizeRef.current?.onMediaResize(bounds);
      onOutputChange?.(
        resizeScreenshotCanvas(
          { height: screenPane.sourceHeight, width: screenPane.sourceWidth },
          gesture.settings,
          bounds,
        ),
      );
      return resizeCameraOverlayCanvas(change.settings, gesture.output, bounds);
    },
    onInteractionEnd: () => {
      endAutoFit();
      onInteractionEnd?.();
      editGesture.endGesture();
    },
    onInteractionStart: () => {
      // Alt can already be down when the press lands, so the gesture carries
      // its own starting canvas from the outset.
      autoFitRef.current = { output, settings: outputSettings, used: false };
      editGesture.beginGesture();
      onInteractionStart?.();
    },
    onSettingsChange,
    onSnapGuidesChange: setSnapGuides,
    screenPane: outputPane,
    settings,
  });
  const inverseScale = "var(--preview-inverse-scale, 1)";
  const radiusHandleOffset = `calc(${(geometry.radius * RADIUS_HANDLE_TRAVEL).toString()}px + ${RADIUS_HANDLE_INSET.toString()}px * ${inverseScale})`;
  const finishInteraction = (event: ReactPointerEvent) => {
    setActiveEdges(null);
    interaction.onPointerUp(event);
  };
  const cancelInteraction = (event: ReactPointerEvent) => {
    setActiveEdges(null);
    interaction.onPointerCancel(event);
  };
  const moveInteraction = (event: ReactPointerEvent) => {
    const point = naturalPoint(event);
    if (activeEdges && point) {
      setMagnifierPoint(
        constrainedHandlePoint(geometry.frame, activeEdges, point),
      );
    }
    interaction.onPointerMove(event);
  };
  const controlInteraction = {
    onPointerCancel: cancelInteraction,
    onPointerMove: moveInteraction,
    onPointerUp: finishInteraction,
  };

  if (nativeWorkspaceEditor) {
    return (
      <NativeRecordingWorkspaceViewport
        ariaLabel="Native baked recording workspace preview"
        isBusy={isBusy}
        isSelecting={tool === "select"}
        panes={[
          {
            height: output.height,
            index: 0,
            label: "Composed recording preview",
            ref: screenCanvasRef,
            width: output.width,
            x: 0,
            y: 0,
          },
        ]}
        workspaceHeight={output.height}
        workspaceWidth={output.width}
      />
    );
  }

  return (
    <InteractivePreviewViewport<HTMLDivElement>
      getMediaSize={() => ({
        height: output.height,
        width: output.width,
      })}
      mediaSizeKey={`${output.width.toString()}x${output.height.toString()}`}
      onZoomChange={onZoomChange}
      renderMedia={({
        onMediaResize,
        onMediaResizeEnd,
        onMediaResizeStart,
        ref,
        style,
      }) => (
        <div
          className="relative shrink-0 select-none"
          onContextMenu={(event) => {
            onTrackContextMenu?.("primary", event);
          }}
          ref={(element) => {
            outputRef.current = element;
            mediaResizeRef.current = {
              onMediaResize,
              onMediaResizeEnd,
              onMediaResizeStart,
            };
            ref(element);
          }}
          style={{
            ...style,
            height: `${output.height.toString()}px`,
            width: `${output.width.toString()}px`,
          }}
        >
          <canvas
            aria-label="Native composed recording preview"
            className={`absolute inset-0 size-full max-w-none ${nativeSurface ? "opacity-0" : ""}`}
            ref={screenCanvasRef}
            role="img"
          />
          <canvas className="hidden" ref={cameraCanvasRef} />
          <ScreenshotPreviewLayer
            isCropTarget={
              interactionEnabled && tool === "crop" && activeTrack !== "primary"
            }
            isEditing={outputControlsVisible && tool === "crop"}
            isItemSelected={activeTrack === "primary"}
            isSelecting={tool === "select"}
            onItemSelect={() => {
              onSelectTrack?.("primary");
            }}
            onOutputChange={onOutputChange}
            onRadiusChange={(radiusPercent) =>
              onOutputChange?.({ ...outputSettings, radiusPercent })
            }
            output={output}
            outputRef={outputRef}
            previewCanvasRef={screenCanvasRef}
            radiusPercent={outputSettings.radiusPercent}
            settings={outputSettings}
            source={{
              height: screenPane.sourceHeight,
              width: screenPane.sourceWidth,
            }}
          />
          {interactionEnabled && tool === "canvas" ? (
            <ScreenshotCanvasControl
              items={[
                {
                  height: screenPane.sourceHeight,
                  id: 0,
                  width: screenPane.sourceWidth,
                },
              ]}
              mediaRef={outputRef}
              onBoundsChange={(bounds) => {
                onMediaResize(bounds);
                const resize = outputResizeRef.current;
                if (resize)
                  onSettingsChange?.(
                    resizeCameraOverlayCanvas(
                      resize.settings,
                      resize.output,
                      bounds,
                    ),
                  );
              }}
              // Every pointer move commits only to the local resize draft: the
              // export window's global state re-renders the whole editor, which
              // starves the native pane's layout loop mid-drag.
              onChange={(next) => {
                onCanvasResizeDraft?.(screenshotWorkspaceItemOutput(next, 0));
              }}
              onResizeEnd={(next) => {
                onMediaResizeEnd();
                outputResizeRef.current = null;
                onInteractionEnd?.();
                onOutputChange?.(screenshotWorkspaceItemOutput(next, 0));
                onCanvasResizeDraft?.(null);
              }}
              onResizeStart={() => {
                onSelectTrack?.("primary");
                outputResizeRef.current = {
                  output,
                  settings,
                };
                onInteractionStart?.();
                onMediaResizeStart();
              }}
              output={output}
              settings={outputWorkspace}
            />
          ) : null}
          <div className="pointer-events-none absolute inset-0" ref={mediaRef}>
            {snapGuides.x ? (
              <div
                className="absolute top-0 h-full bg-warning"
                style={{
                  left: snapGuides.x.value,
                  width: `calc(1px * ${inverseScale})`,
                }}
              />
            ) : null}
            {snapGuides.y ? (
              <div
                className="absolute left-0 w-full bg-warning"
                style={{
                  height: `calc(1px * ${inverseScale})`,
                  top: snapGuides.y.value,
                }}
              />
            ) : null}
            {controlsVisible && tool === "crop" ? (
              <CropShade
                crop={geometry.frame}
                image={geometry.camera}
                radius={geometry.radius}
              />
            ) : null}
            <div
              aria-label="Camera crop window"
              className={`absolute touch-none overflow-hidden ${interactionEnabled && (controlsVisible || tool === "select" || tool === "crop") ? "pointer-events-auto cursor-move" : "pointer-events-none"}`}
              onContextMenu={(event) => {
                event.stopPropagation();
                onTrackContextMenu?.("camera", event);
              }}
              onPointerDown={(event) => {
                // Selecting is part of the drag gesture, like the screen layer:
                // one press both picks the camera up and starts moving it.
                if (
                  (tool === "select" || tool === "crop") &&
                  activeTrack !== "camera"
                ) {
                  if (event.button !== 0) return;
                  onSelectTrack?.("camera");
                }
                if (!controlsVisible && tool !== "crop" && tool !== "select")
                  return;
                const point = naturalPoint(event);
                if (!point) return;
                begin(event, {
                  kind: tool === "crop" ? "frame" : "whole",
                  pointerX: point.x - geometry.frame.x,
                  pointerY: point.y - geometry.frame.y,
                });
              }}
              role="group"
              style={{
                borderRadius: `${geometry.radius.toString()}px`,
                height: `${geometry.frame.height.toString()}px`,
                left: `${geometry.frame.x.toString()}px`,
                top: `${geometry.frame.y.toString()}px`,
                width: `${geometry.frame.width.toString()}px`,
              }}
              {...interaction}
            ></div>
            {controlsVisible ? (
              <TransformControls
                frame={geometry.frame}
                interaction={controlInteraction}
                inverseScale={inverseScale}
                lineStyle={tool === "crop" ? "dashed" : "solid"}
                radius={geometry.radius}
                radiusHandle={
                  tool === "crop"
                    ? {
                        cursor: "nwse-resize",
                        label: `Camera corner radius ${Math.round(settings.radiusPercent).toString()} percent`,
                        left: radiusHandleOffset,
                        onPointerDown: (event) => {
                          begin(event, { kind: "radius" });
                        },
                        top: radiusHandleOffset,
                      }
                    : undefined
                }
                resize={{
                  label: (edges) =>
                    `${tool === "crop" ? "Crop" : "Resize"} camera ${edges.join(" ")}`,
                  onPointerDown: (edges) => (event) => {
                    const point = naturalPoint(event);
                    if (!point) return;
                    if (tool === "crop") {
                      setActiveEdges(edges);
                      setMagnifierPoint(
                        constrainedHandlePoint(geometry.frame, edges, point),
                      );
                    }
                    begin(event, {
                      edges,
                      kind: tool === "crop" ? "resize" : "transformResize",
                      pointerX: point.x,
                      pointerY: point.y,
                    });
                  },
                }}
              />
            ) : null}
            <AnimatePresence>
              {controlsVisible &&
              tool === "crop" &&
              activeEdges &&
              cameraCanvasRef.current ? (
                <div
                  className="pointer-events-none absolute"
                  style={{
                    height: geometry.frame.height,
                    left: geometry.frame.x,
                    top: geometry.frame.y,
                    width: geometry.frame.width,
                  }}
                >
                  <ScreenshotCropMagnifier
                    edges={activeEdges}
                    inverseScale={inverseScale}
                    layout={{ crop: geometry.frame, image: geometry.camera }}
                    point={magnifierPoint}
                    source={{
                      height: cameraPane.sourceHeight,
                      width: cameraPane.sourceWidth,
                    }}
                    sourceImage={cameraCanvasRef.current}
                  />
                </div>
              ) : null}
            </AnimatePresence>
          </div>
          {isBusy ? (
            <div className="pointer-events-none absolute inset-0 bg-content/20 backdrop-blur-sm" />
          ) : null}
        </div>
      )}
      resetKey="baked-recording-output"
      zoomPercent={zoomPercent}
    />
  );
}
