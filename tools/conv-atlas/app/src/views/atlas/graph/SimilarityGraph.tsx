/**
 * Similarity graph view: kNN links between conversations laid out by
 * ForceAtlas2; cross-repo bridges stand out. GraphEngine owns sigma and the store
 * subscriptions; React renders only the header, legend and tooltip.
 */
import { useEffect, useRef, useState } from "react";
import { useAtlas } from "../../../core/store";
import { Chip, Tooltip, ViewHeader } from "../../../design/primitives";
import type { LegendEntry } from "../lib/coloring";
import { ColorByControl, ColorLegend, ConversationTip, StageHint, usePlayheadDate } from "../shared";
import { GraphEngine, type GraphStats } from "./engine";

export function SimilarityGraph() {
  const hostRef = useRef<HTMLDivElement>(null);
  const engineRef = useRef<GraphEngine | null>(null);
  const [bridgesOnly, setBridgesOnly] = useState(false);
  const [tip, setTip] = useState<{ guid: string; x: number; y: number } | null>(null);
  const [legend, setLegend] = useState<LegendEntry[]>([]);
  const [stats, setStats] = useState<GraphStats | null>(null);
  const snapshot = useAtlas((s) => s.snapshot);
  const date = usePlayheadDate();

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const engine = new GraphEngine(host, { tooltip: setTip, legend: setLegend, stats: setStats }, { bridgesOnly: false });
    engineRef.current = engine;
    return () => {
      engine.destroy();
      engineRef.current = null;
    };
  }, []);

  useEffect(() => {
    engineRef.current?.setOptions({ bridgesOnly });
  }, [bridgesOnly]);

  const summary = stats
    ? [
        `${stats.nodes.toLocaleString()} conversations · ${stats.edges.toLocaleString()} links, ${stats.crossEdges.toLocaleString()} cross-repo`,
        ...(stats.focusHidden ? ["selection hidden by filters or playhead: showing the whole graph"] : []),
        `up to ${date}`,
        ...(stats.layingOut ? ["laying out…"] : []),
      ].join(" · ")
    : null;

  return (
    <div style={{ position: "absolute", inset: 0, display: "flex", flexDirection: "column" }}>
      <ViewHeader title="Similarity graph" summary={summary}>
        <ColorByControl />
        <Chip selected={bridgesOnly} onClick={() => setBridgesOnly(!bridgesOnly)} title="Show only links between different repos">
          Bridges only
        </Chip>
        <button type="button" className="ca-btn" data-variant="ghost" onClick={() => engineRef.current?.relayout()} disabled={stats?.layingOut}>
          Re-run layout
        </button>
      </ViewHeader>
      <div style={{ position: "relative", flex: 1, minHeight: 0, overflow: "hidden" }}>
        <div ref={hostRef} style={{ position: "absolute", inset: 0 }} />
        <ColorLegend entries={legend} />
        <StageHint>Click a conversation to focus its neighbours · scroll to zoom · click empty space to clear</StageHint>
      </div>
      <Tooltip at={tip}>{tip && snapshot ? <ConversationTip guid={tip.guid} snapshot={snapshot} /> : null}</Tooltip>
    </div>
  );
}
