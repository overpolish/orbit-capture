<!-- SPDX-FileCopyrightText: 2026 overpolish -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Native preview viewer

The native GPU compositor is the only preview path on every shipped platform.
The DOM/canvas viewer it replaced - `InteractivePreviewViewport`, the DOM
on-screen controls (selection, crop, radius, canvas resize, snapping,
magnifier), the RGBA frame channel and the `preview_capabilities` probe - has
been removed from the frontend.

What the WebKit side still contributes is a passive frame: marker elements
whose bounds define the native workarea and pane rects, plus the composited
backdrop colour and its mask holes. `useRecordingPreviewSurface` and
`useScreenshotPreviewSurface` measure those markers and forward the geometry,
composition and selection state to Rust; everything painted inside the viewport
comes back from `preview_platform` (see `workspace_editor.rs`), including
selection chrome, crop shade, corner-radius handles, snapping and the
pointer/wheel gestures behind pan and zoom.

Consequences worth remembering:

- The viewport is empty DOM. In Storybook, where no backend exists, it renders
  as a labelled placeholder (`.storybook/styles.css`).
- Zoom is native-authoritative: React sends `set_*_preview_zoom` and listens to
  `recording-preview://transform` for the echo.
- Layer edits arrive as `selection-gesture` events and are translated back into
  output settings by the React gesture handlers, which own undo history.
