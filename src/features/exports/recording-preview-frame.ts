// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RecordingPreviewLayout } from "./types";

const FRAME_HEADER_LENGTH = 16;
const NATIVE_FRAME_HEADER_LENGTH = 24;
const NATIVE_FRAME_MARKER = 0x4650434f;

const drawRegion = (
  bitmap: ImageBitmap,
  canvas: HTMLCanvasElement | null,
  region: { height: number; width: number; x: number; y: number },
) => {
  if (!canvas) return;
  if (canvas.width !== region.width) canvas.width = region.width;
  if (canvas.height !== region.height) canvas.height = region.height;
  canvas
    .getContext("2d", { alpha: false })
    ?.drawImage(
      bitmap,
      region.x,
      region.y,
      region.width,
      region.height,
      0,
      0,
      region.width,
      region.height,
    );
};

const drawPane = ({
  bitmap,
  canvas,
  height,
  width,
}: {
  bitmap: ImageBitmap;
  canvas: HTMLCanvasElement | null;
  height: number;
  width: number;
}) => {
  if (!canvas) return;
  if (canvas.width !== width) canvas.width = width;
  if (canvas.height !== height) canvas.height = height;
  canvas
    .getContext("2d", { alpha: false })
    ?.drawImage(bitmap, 0, 0, width, height);
};

const drawNativeFrame = async ({
  camera,
  frame,
  isCurrentRequest,
  layout,
  requestId,
  screen,
}: {
  camera: HTMLCanvasElement | null;
  frame: ArrayBuffer;
  isCurrentRequest: (requestId: number) => boolean;
  layout: RecordingPreviewLayout;
  requestId: number;
  screen: HTMLCanvasElement | null;
}) => {
  if (frame.byteLength < NATIVE_FRAME_HEADER_LENGTH) return false;
  const header = new DataView(frame, 0, NATIVE_FRAME_HEADER_LENGTH);
  const screenLength = header.getUint32(16, true);
  const cameraLength = header.getUint32(20, true);
  const screenEnd = NATIVE_FRAME_HEADER_LENGTH + screenLength;
  const cameraEnd = screenEnd + cameraLength;
  if (screenLength === 0 || cameraEnd > frame.byteLength) return false;
  const screenBitmap = await createImageBitmap(
    new Blob([frame.slice(NATIVE_FRAME_HEADER_LENGTH, screenEnd)], {
      type: "image/jpeg",
    }),
  );
  const cameraBitmap =
    cameraLength > 0
      ? await createImageBitmap(
          new Blob([frame.slice(screenEnd, cameraEnd)], {
            type: "image/jpeg",
          }),
        )
      : null;
  if (!isCurrentRequest(requestId)) {
    screenBitmap.close();
    cameraBitmap?.close();
    return false;
  }
  const screenPane = layout.panes[0];
  drawPane({
    bitmap: screenBitmap,
    canvas: screen,
    height: screenPane.height,
    width: screenPane.width,
  });
  const cameraPane = layout.panes.find((pane) => pane.kind === "camera");
  if (cameraBitmap && cameraPane)
    drawPane({
      bitmap: cameraBitmap,
      canvas: camera,
      height: cameraPane.height,
      width: cameraPane.width,
    });
  screenBitmap.close();
  cameraBitmap?.close();
  return true;
};

export const drawRecordingPreviewFrame = async ({
  camera,
  frame,
  isCurrentRequest,
  layout,
  screen,
}: {
  camera: HTMLCanvasElement | null;
  frame: ArrayBuffer;
  isCurrentRequest: (requestId: number) => boolean;
  layout: RecordingPreviewLayout;
  screen: HTMLCanvasElement | null;
}) => {
  if (frame.byteLength <= FRAME_HEADER_LENGTH) return false;
  const header = new DataView(frame, 0, FRAME_HEADER_LENGTH);
  const encodedWidth = header.getUint32(0, true);
  const encodedHeight = header.getUint32(4, true);
  const requestId = Number(header.getBigUint64(8, true));
  if (encodedWidth === NATIVE_FRAME_MARKER && encodedHeight === 1)
    return drawNativeFrame({
      camera,
      frame,
      isCurrentRequest,
      layout,
      requestId,
      screen,
    });
  if (encodedWidth === 0 || encodedHeight === 0) return false;
  const bitmap = await createImageBitmap(
    new Blob([frame.slice(FRAME_HEADER_LENGTH)], { type: "image/jpeg" }),
  );
  if (!isCurrentRequest(requestId)) {
    bitmap.close();
    return false;
  }
  layout.panes.forEach((pane, index) => {
    drawRegion(bitmap, index === 0 ? screen : camera, {
      height: Math.round((pane.height * encodedHeight) / layout.height),
      width: Math.round((pane.width * encodedWidth) / layout.width),
      x: Math.round((pane.x * encodedWidth) / layout.width),
      y: Math.round((pane.y * encodedHeight) / layout.height),
    });
  });
  bitmap.close();
  return true;
};
