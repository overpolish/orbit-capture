// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect, useMemo, useRef, useState } from "react";

import { cn } from "../../../lib/styling";
import { RecordingTimelineThumbnail } from "../types";

const DEFAULT_THUMBNAIL_ASPECT_RATIO = 16 / 9;

const evenlySampleThumbnails = (
  thumbnails: RecordingTimelineThumbnail[],
  count: number,
) => {
  if (count >= thumbnails.length) return thumbnails;
  if (count === 1) return [thumbnails[Math.floor(thumbnails.length / 2)]];
  return Array.from(
    { length: count },
    (_, index) =>
      thumbnails[Math.round((index * (thumbnails.length - 1)) / (count - 1))],
  );
};

export function VideoThumbnailStrip({
  enabled,
  thumbnails,
}: {
  enabled: boolean;
  thumbnails: RecordingTimelineThumbnail[];
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [containerSize, setContainerSize] = useState({ height: 0, width: 0 });
  const [thumbnailAspectRatio, setThumbnailAspectRatio] = useState(
    DEFAULT_THUMBNAIL_ASPECT_RATIO,
  );

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const observer = new ResizeObserver(([entry]) => {
      const { height, width } = entry.contentRect;
      setContainerSize((current) =>
        current.height === height && current.width === width
          ? current
          : { height, width },
      );
    });
    observer.observe(container);
    return () => {
      observer.disconnect();
    };
  }, []);

  const visibleThumbnails = useMemo(() => {
    if (
      thumbnails.length === 0 ||
      containerSize.height <= 0 ||
      containerSize.width <= 0
    )
      return thumbnails;
    const frameWidth = containerSize.height * thumbnailAspectRatio;
    const count = Math.max(
      1,
      Math.min(thumbnails.length, Math.round(containerSize.width / frameWidth)),
    );
    return evenlySampleThumbnails(thumbnails, count);
  }, [containerSize, thumbnailAspectRatio, thumbnails]);

  return (
    <div
      className={cn(
        "absolute inset-0 flex overflow-hidden transition-[filter,opacity]",
        !enabled && "opacity-35 grayscale",
      )}
      ref={containerRef}
    >
      {visibleThumbnails.length > 0 ? (
        visibleThumbnails.map((thumbnail) =>
          thumbnail.url ? (
            <img
              aria-hidden="true"
              className="h-full min-w-0 flex-1 object-contain"
              key={thumbnail.id}
              onLoad={(event) => {
                const { naturalHeight, naturalWidth } = event.currentTarget;
                if (naturalHeight <= 0 || naturalWidth <= 0) return;
                const nextAspectRatio = naturalWidth / naturalHeight;
                setThumbnailAspectRatio((current) =>
                  Math.abs(current - nextAspectRatio) < 0.001
                    ? current
                    : nextAspectRatio,
                );
              }}
              src={thumbnail.url}
            />
          ) : (
            <span
              aria-hidden="true"
              className="h-full min-w-0 flex-1 bg-muted/8"
              key={thumbnail.id}
            />
          ),
        )
      ) : (
        <span className="size-full bg-muted/8" />
      )}
    </div>
  );
}
