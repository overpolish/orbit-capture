import { useEffect, useRef, useState } from "react";

import { elapsedMilliseconds } from "./elapsed-time";
import { RecordingSnapshot } from "./types";

const TICK_INTERVAL_MS = 100;

export function useElapsedTime(snapshot: RecordingSnapshot) {
  const [now, setNow] = useState(() => Date.now());
  const displayedRef = useRef(0);

  useEffect(() => {
    // While paused or finishing the time is frozen and `now` is ignored, so
    // there is nothing to tick.
    if (snapshot.status !== "recording") return;

    const interval = window.setInterval(() => {
      setNow(Date.now());
    }, TICK_INTERVAL_MS);

    return () => {
      window.clearInterval(interval);
    };
  }, [snapshot.status]);

  const elapsedMs = elapsedMilliseconds(snapshot, now);

  // Rust folds the pause span using its own reading of the wall clock, and the
  // paused snapshot reaches this window a few milliseconds later. The last tick
  // before it arrives has therefore already counted past the frozen total, so
  // the raw value dips on arrival - and across a second boundary that dip is a
  // visible backwards tick. Ratcheting within a session prevents it.
  if (snapshot.status === "idle" || snapshot.status === "starting") {
    displayedRef.current = 0;
  } else if (elapsedMs > displayedRef.current) {
    displayedRef.current = elapsedMs;
  }

  return displayedRef.current;
}
