// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RecordingPreviewLayout } from "./types";

const FRAME_HEADER_LENGTH = 16;
const NATIVE_FRAME_HEADER_LENGTH = 24;
const NATIVE_FRAME_CURSOR_HEADER_LENGTH = 44;
const NATIVE_FRAME_MARKER = 0x4650434f;
const NATIVE_FRAME_VERSION = 2;

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
}: {
  bitmap: ImageBitmap;
  canvas: HTMLCanvasElement | null;
}) => {
  if (!canvas) return;
  // The pane's CSS size describes its place in the preview layout. Keep the
  // canvas backing store at the resolution supplied by the native decoder so
  // a source-resolution still is not immediately reduced back to 720p.
  if (canvas.width !== bitmap.width) canvas.width = bitmap.width;
  if (canvas.height !== bitmap.height) canvas.height = bitmap.height;
  canvas
    .getContext("2d", { alpha: false })
    ?.drawImage(bitmap, 0, 0, bitmap.width, bitmap.height);
};

const drawNativeFrame = async ({
  camera,
  cursor,
  frame,
  isCurrentRequest,
  layout,
  requestId,
  screen,
}: {
  camera: HTMLCanvasElement | null;
  cursor: HTMLCanvasElement | null;
  frame: ArrayBuffer;
  isCurrentRequest: (requestId: number) => boolean;
  layout: RecordingPreviewLayout;
  requestId: number;
  screen: HTMLCanvasElement | null;
}) => {
  if (frame.byteLength < NATIVE_FRAME_HEADER_LENGTH) return false;
  const initialHeader = new DataView(frame, 0, NATIVE_FRAME_HEADER_LENGTH);
  const version = initialHeader.getUint32(4, true);
  if (version < 1 || version > NATIVE_FRAME_VERSION) return false;
  const headerLength =
    version >= 2
      ? NATIVE_FRAME_CURSOR_HEADER_LENGTH
      : NATIVE_FRAME_HEADER_LENGTH;
  if (frame.byteLength < headerLength) return false;
  const header = new DataView(frame, 0, headerLength);
  const screenLength = header.getUint32(16, true);
  const cameraLength = header.getUint32(20, true);
  const cursorLength = version >= 2 ? header.getUint32(24, true) : 0;
  const cursorX = version >= 2 ? header.getInt32(28, true) : 0;
  const cursorY = version >= 2 ? header.getInt32(32, true) : 0;
  const cursorCanvasWidth = version >= 2 ? header.getUint32(36, true) : 0;
  const cursorCanvasHeight = version >= 2 ? header.getUint32(40, true) : 0;
  const screenEnd = headerLength + screenLength;
  const cameraEnd = screenEnd + cameraLength;
  const cursorEnd = cameraEnd + cursorLength;
  if (screenLength === 0 || cursorEnd > frame.byteLength) return false;
  const screenBitmap = await createImageBitmap(
    new Blob([frame.slice(headerLength, screenEnd)], {
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
  const cursorBitmap =
    cursorLength > 0
      ? await createImageBitmap(
          new Blob([frame.slice(cameraEnd, cursorEnd)], { type: "image/png" }),
        )
      : null;
  if (!isCurrentRequest(requestId)) {
    screenBitmap.close();
    cameraBitmap?.close();
    cursorBitmap?.close();
    return false;
  }
  drawPane({
    bitmap: screenBitmap,
    canvas: screen,
  });
  const cameraPane = layout.panes.find((pane) => pane.kind === "camera");
  if (cameraBitmap && cameraPane)
    drawPane({
      bitmap: cameraBitmap,
      canvas: camera,
    });
  if (cursor) {
    cursor.dataset.sourceHeight = cursorCanvasHeight.toString();
    cursor.dataset.sourceWidth = cursorCanvasWidth.toString();
    const context = cursor.getContext("2d");
    if (cursorBitmap) {
      if (
        cursor.width !== cursorBitmap.width ||
        cursor.height !== cursorBitmap.height
      ) {
        cursor.width = cursorBitmap.width;
        cursor.height = cursorBitmap.height;
      } else {
        context?.clearRect(0, 0, cursor.width, cursor.height);
      }
      cursor.style.height = `${((cursorBitmap.height / cursorCanvasHeight) * 100).toString()}%`;
      cursor.style.left = `${((cursorX / cursorCanvasWidth) * 100).toString()}%`;
      cursor.style.top = `${((cursorY / cursorCanvasHeight) * 100).toString()}%`;
      cursor.style.width = `${((cursorBitmap.width / cursorCanvasWidth) * 100).toString()}%`;
      context?.drawImage(cursorBitmap, 0, 0);
    } else {
      context?.clearRect(0, 0, cursor.width, cursor.height);
    }
  }
  screenBitmap.close();
  cameraBitmap?.close();
  cursorBitmap?.close();
  return true;
};

export const drawRecordingPreviewFrame = async ({
  camera,
  cursor,
  frame,
  isCurrentRequest,
  layout,
  screen,
}: {
  camera: HTMLCanvasElement | null;
  cursor: HTMLCanvasElement | null;
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
  // Native packets use this second word for their protocol version, not an
  // encoded height. Dispatch by the marker and let drawNativeFrame validate
  // the version so adding packet fields cannot accidentally turn it into a
  // monolithic JPEG again.
  if (encodedWidth === NATIVE_FRAME_MARKER)
    return drawNativeFrame({
      camera,
      cursor,
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
