// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Camera, Monitor } from "lucide-react";

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
  playhead,
  selectedTrack,
  thumbnails,
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
  volumes: AudioTrackVolumes;
}) {
  const rowCount = layout.panes.length + audioTracks.length;
  const meterHeight = 16 + rowCount * 34;

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

          {layout.panes.map((pane, index) => {
            const Icon = pane.kind === "camera" ? Camera : Monitor;
            const label = pane.kind === "camera" ? "Camera" : "Screen";
            const trackId: RecordingVideoTrackId =
              index === 0 ? "primary" : "camera";
            const enabled = enabledVideoTracks.has(trackId);
            return (
              <div className="flex items-center gap-2" key={trackId}>
                <div
                  className={`flex h-8 w-36 shrink-0 items-center gap-2 rounded px-2 text-xs font-medium text-content-fg transition-colors ${selectedTrack === trackId ? "bg-info/15" : ""}`}
                  onClick={() => {
                    onSelectedTrackChange(trackId);
                  }}
                >
                  <Checkbox
                    aria-label={`${enabled ? "Exclude" : "Include"} ${label}`}
                    isSelected={enabled}
                    onChange={() => {
                      const next = new Set(enabledVideoTracks);
                      if (next.has(trackId)) next.delete(trackId);
                      else next.add(trackId);
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
    </section>
  );
}
