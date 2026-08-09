// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

export const clamp = (value: number, min: number, max: number) =>
  Math.min(Math.max(value, min), max);

type PlayheadListener = (seconds: number, ratio: number) => void;
export type Playhead = ReturnType<typeof createPlayhead>;

/**
 * The playing position, deliberately kept out of React.
 *
 * Playback moves the position sixty times a second. Held in state, every one
 * of those became a render of this whole subtree - including the viewport,
 * which re-measures and re-applies its fit on every render - so the cost of
 * drawing a moving line was paid across components that had nothing to do
 * with it. Subscribers here write the new position straight to their own
 * element, the same way the viewport already drives its zoom.
 */
export const createPlayhead = () => {
  const listeners = new Set<PlayheadListener>();
  let last = { ratio: 0, seconds: 0 };

  return {
    publish: (seconds: number, ratio: number) => {
      last = { ratio, seconds };
      for (const listener of listeners) listener(seconds, ratio);
    },
    subscribe: (listener: PlayheadListener) => {
      listeners.add(listener);
      // So a subscriber that mounts mid-playback is not left at zero.
      listener(last.seconds, last.ratio);
      return () => {
        listeners.delete(listener);
      };
    },
  };
};
