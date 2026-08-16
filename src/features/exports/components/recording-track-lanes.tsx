// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Camera, Monitor } from "lucide-react";
import { PointerEvent as ReactPointerEvent, useRef, useState } from "react";

import { Checkbox } from "../../../components/base/checkbox/checkbox";
import {
  PreparedAudioTrack,
  recordingAudioStreamIndex,
  recordingAudioTrackId,
  RecordingPreviewLayout,
  RecordingTrackId,
  RecordingTimelineThumbnails,
  RecordingVideoTrackId,
} from "../types";

import { AudioTrackVolumes } from "./audio-level";
import {
  LayerContextMenu,
  LayerContextMenuState,
} from "./screenshot-layer-context-menu";
import { ScrubAudioTracks } from "./scrub-audio-tracks";
import { Playhead } from "./scrub-playhead";
import { SeekHandler, TimelineRuler, TimelineScrubber } from "./scrub-timeline";
import { TimelineAudioMeter } from "./timeline-audio-meter";
import { VideoThumbnailStrip } from "./video-thumbnail-strip";

export function RecordingTrackLanes({
  audioTracks,
  durationMs,
  enabledTracks,
  enabledVideoTracks,
  layout,
  onEnabledTracksChange,
  onEnabledVideoTracksChange,
  onSeek,
  onSelectedTrackChange,
  onVideoTrackOrderChange,
  playhead,
  selectedTrack,
  thumbnails,
  videoTrackOrder,
  volumes,
}: {
  audioTracks: PreparedAudioTrack[];
  durationMs: number;
  enabledTracks: Set<number>;
  enabledVideoTracks: Set<RecordingVideoTrackId>;
  layout: RecordingPreviewLayout;
  onEnabledTracksChange: (tracks: Set<number>) => void;
  onEnabledVideoTracksChange: (tracks: Set<RecordingVideoTrackId>) => void;
  onSeek: SeekHandler;
  onSelectedTrackChange: (trackId: RecordingTrackId) => void;
  playhead: Playhead;
  selectedTrack: RecordingTrackId | null;
  thumbnails: RecordingTimelineThumbnails;
  videoTrackOrder: RecordingVideoTrackId[];
  volumes: AudioTrackVolumes;
  onVideoTrackOrderChange?: (tracks: RecordingVideoTrackId[]) => void;
}) {
  const [contextMenu, setContextMenu] =
    useState<LayerContextMenuState<RecordingVideoTrackId> | null>(null);
  const [drag, setDrag] = useState<{
    dropIndex: number;
    source: RecordingVideoTrackId;
  } | null>(null);
  const dragRef = useRef<{
    dropIndex: number;
    source: RecordingVideoTrackId;
    startY: number;
    started: boolean;
  } | null>(null);
  const rowElementsRef = useRef(
    new Map<RecordingVideoTrackId, HTMLDivElement>(),
  );
  const rowCount = layout.panes.length + audioTracks.length;
  const meterHeight = 16 + rowCount * 34;
  const videoRows = layout.panes
    .map((pane, index) => ({
      pane,
      trackId: index === 0 ? ("primary" as const) : ("camera" as const),
    }))
    .sort(
      (left, right) =>
        videoTrackOrder.indexOf(left.trackId) -
        videoTrackOrder.indexOf(right.trackId),
    );
  const applyOrder = (source: RecordingVideoTrackId, dropIndex: number) => {
    const sourceIndex = videoTrackOrder.indexOf(source);
    if (sourceIndex === -1) return;
    const next = videoTrackOrder.filter((track) => track !== source);
    const insertionIndex = Math.max(
      0,
      Math.min(
        next.length,
        dropIndex > sourceIndex ? dropIndex - 1 : dropIndex,
      ),
    );
    next.splice(insertionIndex, 0, source);
    if (next.some((track, index) => track !== videoTrackOrder[index]))
      onVideoTrackOrderChange?.(next);
  };
  const moveTrack = (
    track: RecordingVideoTrackId,
    direction: "backward" | "forward",
  ) => {
    setContextMenu(null);
    const index = videoTrackOrder.indexOf(track);
    const nextIndex = direction === "forward" ? index - 1 : index + 1;
    if (index === -1 || nextIndex < 0 || nextIndex >= videoTrackOrder.length)
      return;
    const next = [...videoTrackOrder];
    [next[index], next[nextIndex]] = [next[nextIndex], next[index]];
    onVideoTrackOrderChange?.(next);
  };
  const beginDrag =
    (source: RecordingVideoTrackId) =>
    (event: ReactPointerEvent<HTMLDivElement>) => {
      if (event.button !== 0 || videoRows.length < 2) return;
      if (
        event.target instanceof Element &&
        event.target.closest("button, input, [role='checkbox']")
      )
        return;
      dragRef.current = {
        dropIndex: videoTrackOrder.indexOf(source),
        source,
        startY: event.clientY,
        started: false,
      };
      event.currentTarget.setPointerCapture(event.pointerId);
    };
  const updateDrag = (event: ReactPointerEvent<HTMLDivElement>) => {
    const active = dragRef.current;
    if (!active) return;
    if (!active.started && Math.abs(event.clientY - active.startY) <= 4) return;
    active.started = true;
    event.preventDefault();
    let dropIndex = videoRows.length;
    for (let index = 0; index < videoRows.length; index += 1) {
      const row = rowElementsRef.current.get(videoRows[index].trackId);
      if (!row) continue;
      const bounds = row.getBoundingClientRect();
      if (event.clientY < bounds.top + bounds.height / 2) {
        dropIndex = index;
        break;
      }
    }
    active.dropIndex = dropIndex;
    setDrag({ dropIndex, source: active.source });
  };
  const finishDrag = (event: ReactPointerEvent<HTMLDivElement>) => {
    const active = dragRef.current;
    dragRef.current = null;
    if (active?.started) applyOrder(active.source, active.dropIndex);
    setDrag(null);
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
  };
  const cancelDrag = (event: ReactPointerEvent<HTMLDivElement>) => {
    dragRef.current = null;
    setDrag(null);
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
  };

  return (
    <section
      aria-label="Recording timeline"
      className="shrink-0 border-t border-muted/15 bg-content/55 py-2 pr-3 pl-3"
    >
      <div className="flex items-stretch gap-2">
        <div className="relative flex min-w-0 grow flex-col gap-0.5">
          <div className="flex items-center gap-2">
            <span aria-hidden="true" className="w-36 shrink-0" />
            <TimelineRuler
              durationMs={durationMs}
              onSeek={onSeek}
              playhead={playhead}
            />
          </div>

          {videoRows.map(({ pane, trackId }, rowIndex) => {
            const Icon = pane.kind === "camera" ? Camera : Monitor;
            const label = pane.kind === "camera" ? "Camera" : "Screen";
            const enabled = enabledVideoTracks.has(trackId);
            const mustRemainEnabled =
              enabled &&
              enabledVideoTracks.size === 1 &&
              enabledTracks.size === 0;
            return (
              <div
                className={`relative flex items-center gap-2 transition-opacity ${drag?.source === trackId ? "opacity-55" : ""}`}
                key={trackId}
                onContextMenu={(event) => {
                  event.preventDefault();
                  onSelectedTrackChange(trackId);
                  setContextMenu({
                    itemId: trackId,
                    x: Math.min(event.clientX, window.innerWidth - 196),
                    y: Math.min(event.clientY, window.innerHeight - 92),
                  });
                }}
                ref={(element) => {
                  if (element) rowElementsRef.current.set(trackId, element);
                  else rowElementsRef.current.delete(trackId);
                }}
              >
                {drag?.dropIndex === rowIndex ? (
                  <div className="pointer-events-none absolute -top-0.5 right-0 left-0 z-20 h-0.5 rounded bg-info" />
                ) : null}
                {rowIndex === videoRows.length - 1 &&
                drag?.dropIndex === videoRows.length ? (
                  <div className="pointer-events-none absolute -bottom-0.5 right-0 left-0 z-20 h-0.5 rounded bg-info" />
                ) : null}
                <div
                  className={`flex h-8 w-36 shrink-0 cursor-grab items-center gap-2 rounded px-2 text-xs font-medium text-content-fg transition-colors active:cursor-grabbing ${selectedTrack === trackId ? "bg-info/15" : ""}`}
                  onClick={() => {
                    onSelectedTrackChange(trackId);
                  }}
                  onPointerCancel={cancelDrag}
                  onPointerDown={beginDrag(trackId)}
                  onPointerMove={updateDrag}
                  onPointerUp={finishDrag}
                >
                  <Checkbox
                    aria-label={
                      mustRemainEnabled
                        ? `${label} must remain included`
                        : `${enabled ? "Exclude" : "Include"} ${label}`
                    }
                    isDisabled={mustRemainEnabled}
                    isSelected={enabled}
                    onChange={() => {
                      const next = new Set(enabledVideoTracks);
                      if (next.has(trackId)) {
                        if (next.size === 1 && enabledTracks.size === 0) return;
                        next.delete(trackId);
                      } else next.add(trackId);
                      onEnabledVideoTracksChange(next);
                    }}
                    size="xs"
                  />
                  <Icon className="shrink-0 text-muted" size={14} />
                  <span className="min-w-0 grow truncate">{label}</span>
                </div>
                <div
                  aria-selected={selectedTrack === trackId}
                  className="relative h-8 min-w-0 grow cursor-default overflow-hidden rounded bg-muted/8"
                  onClick={() => {
                    onSelectedTrackChange(trackId);
                  }}
                >
                  <VideoThumbnailStrip
                    enabled={enabled}
                    thumbnails={thumbnails[trackId]}
                  />
                </div>
              </div>
            );
          })}

          {audioTracks.length > 0 ? (
            <ScrubAudioTracks
              audioTracks={audioTracks}
              enabledTracks={enabledTracks}
              hasEnabledVideo={enabledVideoTracks.size > 0}
              onEnabledTracksChange={onEnabledTracksChange}
              onSelectTrack={(streamIndex) => {
                onSelectedTrackChange(recordingAudioTrackId(streamIndex));
              }}
              selectedTrack={recordingAudioStreamIndex(selectedTrack)}
              volumes={volumes}
            />
          ) : null}

          <div className="pointer-events-none absolute inset-y-0 right-0 left-[9.5rem] z-10 overflow-hidden">
            <TimelineScrubber onSeek={onSeek} playhead={playhead} />
          </div>
        </div>

        {audioTracks.length > 0 ? (
          <TimelineAudioMeter
            audioTracks={audioTracks}
            enabledTracks={enabledTracks}
            height={meterHeight}
            playhead={playhead}
            volumes={volumes}
          />
        ) : null}
      </div>
      {contextMenu ? (
        <LayerContextMenu
          ariaLabel="Video layer actions"
          canDelete={false}
          menu={contextMenu}
          onClose={() => {
            setContextMenu(null);
          }}
          onDelete={() => undefined}
          onMoveBackward={() => {
            moveTrack(contextMenu.itemId, "backward");
          }}
          onMoveForward={() => {
            moveTrack(contextMenu.itemId, "forward");
          }}
          showDelete={false}
        />
      ) : null}
    </section>
  );
}
