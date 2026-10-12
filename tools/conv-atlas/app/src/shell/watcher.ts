/**
 * Watcher health, derived from the store: core/runtime.ts calls setManifest on
 * every successful poll, so a fresh manifest object = a live watcher.
 */
import { useEffect, useState } from "react";
import { WATCH_INTERVAL_MS } from "../core/runtime";
import { atlas } from "../core/store";

let lastPoll = 0;
atlas.subscribe(
  (s) => s.manifest,
  (m) => {
    if (m) lastPoll = Date.now();
  },
);

export interface WatcherHealth {
  state: "live" | "stale" | "waiting";
  title: string;
}

export function useWatcherHealth(): WatcherHealth {
  const [, tick] = useState(0);
  useEffect(() => {
    const id = setInterval(() => tick((n) => n + 1), 5000);
    return () => clearInterval(id);
  }, []);
  if (!lastPoll) return { state: "waiting", title: "Waiting for the first manifest check" };
  const age = Date.now() - lastPoll;
  const secs = Math.round(age / 1000);
  return age > WATCH_INTERVAL_MS * 2.5
    ? { state: "stale", title: `Not checking for new data: last check ${secs} s ago. Is the data server running?` }
    : { state: "live", title: `Watching for new snapshots (checked ${secs} s ago)` };
}
