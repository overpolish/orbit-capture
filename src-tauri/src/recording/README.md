<!--
SPDX-FileCopyrightText: 2026 overpolish
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Recording

```mermaid
flowchart TD
  BAR["Recording bar"] --> OPTIONS["What to record"]
  OPTIONS --> SHARED["Shared recording flow"]

  SHARED --> START["Start"]
  SHARED --> CONTROL["Pause / resume"]
  SHARED --> FINISH["Stop / cancel"]

  START --> PLATFORM{"Platform recording"}
  CONTROL --> PLATFORM
  FINISH --> PLATFORM

  PLATFORM -->|"macOS"| MAC["ScreenCaptureKit + AVFoundation"]
  PLATFORM -.->|"Windows TODO"| WINDOWS["Windows Graphics Capture + Media Foundation + WASAPI"]

  MAC --> FILES["Video and audio tracks"]
  WINDOWS -.-> FILES
  MAC --> CURSOR["Shared cursor sidecar"]
  WINDOWS -.-> CURSOR
  FILES --> PREVIEW["Rust preview"]
  FILES --> EXPORT["Export / copy / cleanup"]
  CURSOR --> EXPORT
```

## Shared

There is one recording flow for both platforms. It handles:

- modes and selected sources
- start, pause, resume, stop and cancel
- recording state
- working files and cleanup
- timing and export info

The platform part only needs to provide:

- `begin_blocking(config)` to start capture
- `CaptureSession` with pause, resume, stop and cancel
- `CaptureStart` with the first written frame, shared clock, source scale and cursor bounds

The platform is picked at compile time. It is not a plugin system.

## macOS

```mermaid
flowchart LR
  CONFIG["What to record"] --> SCK["ScreenCaptureKit"]
  CONFIG --> AVF["AVFoundation"]
  CONFIG --> POINTER["Core Graphics events + AppKit cursor"]

  SCK --> SCREEN["Screen / region / window"]
  SCK --> AUDIO["All or selected app audio"]
  AVF --> CAMERA["Camera"]
  AVF --> MIC["Microphone"]

  SCREEN --> WRITERS["Writer threads"]
  AUDIO --> WRITERS
  CAMERA --> WRITERS
  MIC --> WRITERS
  POINTER --> CURSOR["Cursor JSONL"]

  WRITERS --> PRIMARY["Main recording"]
  WRITERS --> CAMERA_FILE["Camera file when needed"]
```

- Screen, region and window use ScreenCaptureKit.
- System audio and selected app audio use ScreenCaptureKit, including audio only mode.
- Camera and microphone use AVFoundation.
- AVAssetWriter writes the files and uses hardware video encoding.
- Camera stays separate unless bake in is enabled on export.
- Microphone stays on its own track so it is easy to edit later.
- Cursor position, appearance and button changes use the same recording clock as the media writers. Pauses are removed once in the shared cursor writer.
- Cursor files use global logical coordinates and include the captured source bounds. The macOS part only translates native events and cursor styles into the shared format.

## Windows

Windows recording is TODO. It will use the same shared flow and implement the platform part with:

- Windows Graphics Capture for screen, region and window
- Media Foundation for camera and hardware encoding
- WASAPI for microphone, system audio and app audio
- Raw Input plus Win32 cursor inspection for the shared cursor format

The recording bar, recording state and export UI should not need Windows versions.
