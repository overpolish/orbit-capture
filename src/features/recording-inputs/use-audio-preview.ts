// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Channel } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";

import {
  AudioPreviewEvent,
  AudioPreviewKind,
  startAudioPreview,
  stopAudioPreview,
} from "./audio-preview-api";

const usePeak = (decibels: number) => {
  const [peak, setPeak] = useState(-Infinity);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (!Number.isFinite(decibels)) return;

    setPeak((current) => {
      if (decibels <= current) return current;
      if (timeoutRef.current) clearTimeout(timeoutRef.current);
      timeoutRef.current = setTimeout(() => {
        setPeak(-Infinity);
      }, 3000);
      return decibels;
    });
  }, [decibels]);

  useEffect(
    () => () => {
      if (timeoutRef.current) clearTimeout(timeoutRef.current);
    },
    [],
  );

  return Number.isFinite(decibels) ? peak : -Infinity;
};

type UseAudioPreviewOptions = {
  active: boolean;
  kind: AudioPreviewKind;
  applicationIds?: string[];
  deviceId?: string;
  processIds?: number[];
};

export const useAudioPreview = ({
  active,
  applicationIds,
  deviceId,
  kind,
  processIds,
}: UseAudioPreviewOptions) => {
  const [decibels, setDecibels] = useState(-Infinity);
  const operationsRef = useRef(Promise.resolve());

  useEffect(() => {
    let cancelled = false;
    operationsRef.current = operationsRef.current
      .then(async () => {
        setDecibels(-Infinity);
        await stopAudioPreview(kind);
        if (!active || cancelled) return;

        const channel = new Channel<AudioPreviewEvent>();
        channel.onmessage = (message) => {
          if (cancelled) return;
          if (message.event === "signal") setDecibels(message.data.decibels);
          else setDecibels(-Infinity);
        };
        await startAudioPreview({
          applicationIds,
          channel,
          deviceId,
          kind,
          processIds,
        });
      })
      .catch(() => {
        if (!cancelled) setDecibels(-Infinity);
      });

    return () => {
      cancelled = true;
      setDecibels(-Infinity);
      operationsRef.current = operationsRef.current
        .then(() => stopAudioPreview(kind))
        .catch(() => undefined);
    };
  }, [active, applicationIds, deviceId, kind, processIds]);

  return { decibels, peak: usePeak(decibels) };
};
