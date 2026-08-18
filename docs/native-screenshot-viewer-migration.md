<!-- SPDX-FileCopyrightText: 2026 overpolish -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Native screenshot viewer migration

The native screenshot path no longer mounts `InteractivePreviewViewport` or
its WebKit editing controls. The WebKit side contributes only a passive frame
whose bounds define the native AppKit/Metal workarea.

## Implemented natively

- Fit the composed screenshot into the workarea with the existing 8-point gutter.
- Left- or middle-button drag to pan.
- Trackpad/two-axis wheel pan.
- Trackpad pinch and Control-wheel pointer-anchored zoom.
- Toolbar-controlled zoom from 10% to 1600%.
- Double-click reset to fit.
- Live toolbar zoom updates from native gestures.

## Still to reimplement natively

- Layer selection, deselection, ordering, and context menus.
- Screenshot move, frame resize, crop, and crop shade.
- Canvas resize, including centred Alt resize and anchor compensation.
- Screenshot and workspace corner-radius controls.
- Snapping, snap guides, modifier tracking, and Alt workspace auto-fit.
- Crop pixel magnifier.
- Full-resolution source escalation above fit zoom.
- Tool-specific cursors and gesture cancellation/pointer-capture semantics.
- Accessibility equivalents for transform handles, radius controls, and menus.
- Animated reset and any desired inertial/elastic gesture behaviour.

## Removed WebKit coupling

- Per-frame `requestAnimationFrame` geometry measurement.
- CSS `zoom` and translated media-host transforms.
- Pointer and wheel listeners in `InteractivePreviewViewport`.
- `screenwide-preview-transformed` and
  `screenwide-preview-transform-committed` synchronization.
- DOM screenshot OSC rendering on the native path.
