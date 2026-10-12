/**
 * Runtime loops owned by core: the playback clock and the manifest/insight watcher.
 * Started once from main.tsx; views never run their own clocks.
 */
import { loadInsightIndex, loadManifest } from "./data";
import { DAY_MS } from "./semantics";
import { atlas } from "./store";

/** Advances time.cursorMs while playing; stops (or loops) at the end of the range. */
export function startPlaybackClock(): () => void {
  let last = performance.now();
  let raf = 0;
  const tick = (now: number) => {
    const dt = Math.min(now - last, 100) / 1000; // clamp: a backgrounded tab must not jump
    last = now;
    const { time, setTime } = atlas.getState();
    if (time.playing) {
      const [start, end] = time.range;
      let cursor = time.cursorMs + dt * time.speed * DAY_MS;
      if (cursor >= end) {
        if (time.loop) cursor = start;
        else {
          setTime({ cursorMs: end, playing: false });
          raf = requestAnimationFrame(tick);
          return;
        }
      }
      setTime({ cursorMs: cursor });
    }
    raf = requestAnimationFrame(tick);
  };
  raf = requestAnimationFrame(tick);
  return () => cancelAnimationFrame(raf);
}

export const WATCH_INTERVAL_MS = 15_000;

/**
 * Long-lived page: poll the manifest and the insight index. The first manifest
 * opens its latest snapshot; later ones only announce `pending`, which the
 * user accepts, so a swap never happens under someone's cursor.
 */
export function startWatcher(intervalMs = WATCH_INTERVAL_MS): () => void {
  let stopped = false;
  const poll = async () => {
    const s = atlas.getState();
    try {
      const manifest = await loadManifest();
      s.setManifest(manifest);
      if (!atlas.getState().snapshot && !atlas.getState().loading) await s.openSnapshot(manifest.latest);
    } catch (e) {
      if (!s.snapshot) atlas.setState({ error: e instanceof Error ? e.message : String(e) });
    }
    s.setInsights(await loadInsightIndex());
  };
  void poll();
  const id = setInterval(() => {
    if (!stopped) void poll();
  }, intervalMs);
  return () => {
    stopped = true;
    clearInterval(id);
  };
}
