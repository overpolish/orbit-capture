// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

export type AudioTrackVolumes = ReadonlyMap<number, number>;

export const decibelGain = (decibels: number) =>
  decibels <= -60 ? 0 : 10 ** (decibels / 20);

export const trackGain = (streamIndex: number, volumes: AudioTrackVolumes) =>
  decibelGain(volumes.get(streamIndex) ?? 0);
