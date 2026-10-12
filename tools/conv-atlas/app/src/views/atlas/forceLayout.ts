/**
 * Bounded ForceAtlas2 for the sigma views. The worker iterates on a shadow copy
 * of the graph, and settled positions are copied into the rendered graph at
 * most every SYNC_MS. Streaming every iteration straight into the sigma graph
 * made sigma reprocess the whole graph per iteration, which blocked the main
 * thread for hundreds of ms at a time. The spell is time-bounded so an all-day
 * page does not keep a core busy.
 *
 * Under reduced motion nothing moves on screen: the stage elements are hidden
 * for a short, undamped spell and the settled positions land in one update.
 */
import type Graph from "graphology";
import { inferSettings } from "graphology-layout-forceatlas2";
import FA2Layout from "graphology-layout-forceatlas2/worker";

const REDUCED_MOTION = "(prefers-reduced-motion: reduce)";
const SYNC_MS = 120;
/** off-screen spell under reduced motion: the stage is blank meanwhile */
const OFFSCREEN_MS = 600;

/**
 * Starts the layout; returns a stop function, or null when the graph is empty
 * and nothing ran. `onStop` runs once, after the final positions are applied,
 * whether the spell timed out or was stopped. `stage` is what reduced motion
 * hides until then (the renderer and anything drawn over it).
 */
export function runForceLayout(graph: Graph, ms: number, stage: readonly HTMLElement[], onStop: () => void): (() => void) | null {
  if (graph.order === 0) return null;
  const offscreen = matchMedia(REDUCED_MOTION).matches;
  const shadow = graph.copy();
  const layout = new FA2Layout(shadow, {
    // damped on screen so motion reads smoothly; off screen, FA2's default (undamped) pace converges in fewer iterations
    settings: { ...inferSettings(shadow), barnesHutOptimize: shadow.order > 400, slowDown: offscreen ? 1 : 8, edgeWeightInfluence: 1 },
    getEdgeWeight: "weight",
  });
  const sync = () => {
    graph.updateEachNodeAttributes((key, attrs) => {
      const s = shadow.getNodeAttributes(key);
      return { ...attrs, x: s.x, y: s.y };
    });
  };
  layout.start();
  if (offscreen) for (const el of stage) el.style.visibility = "hidden";
  const ticker = offscreen ? 0 : window.setInterval(sync, SYNC_MS);
  let stopped = false;
  const stop = () => {
    if (stopped) return;
    stopped = true;
    window.clearTimeout(timer);
    window.clearInterval(ticker);
    layout.kill();
    sync();
    if (offscreen) for (const el of stage) el.style.visibility = "";
    onStop();
  };
  const timer = window.setTimeout(stop, offscreen ? Math.min(ms, OFFSCREEN_MS) : ms);
  return stop;
}
