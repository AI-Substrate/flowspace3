/**
 * Galaxy view: the conversation map. React owns only the chrome (header,
 * legend, tooltip); GalaxyEngine owns the canvases and listens to the store
 * directly, so playback never re-renders this component per frame.
 */
import { useEffect, useRef, useState } from "react";
import { NO_FILTERS } from "../../../core/semantics";
import { useAtlas } from "../../../core/store";
import { Chip, EmptyState, SegmentedControl, Tooltip, ViewHeader } from "../../../design/primitives";
import type { LegendEntry } from "../lib/coloring";
import { ColorByControl, ColorLegend, ConversationTip, StageHint, usePlayheadDate } from "../shared";
import { type EdgeMode, GalaxyEngine, type GalaxyStats, type TooltipAnchor } from "./engine";

const EDGE_OPTIONS: { value: EdgeMode; label: string; title: string }[] = [
  { value: "off", label: "No edges", title: "Hide similarity edges" },
  { value: "all", label: "kNN", title: "Nearest-neighbour edges born by the playhead" },
  { value: "cross", label: "Cross-repo", title: "Only edges linking two different repos" },
];

/**
 * Keyboard path to the pointer gestures: C picks the selection's cluster (the
 * lasso), Z zooms to it (double-click). Skipped while typing, in a dialog, or
 * when the shell already handled the key (e.g. the second key of G+key).
 */
function onStageKey(engine: GalaxyEngine, e: KeyboardEvent): void {
  const t = e.target instanceof Element ? e.target : null;
  if (e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return;
  if (t?.closest("input, textarea, select, dialog, [contenteditable='true']")) return;
  const key = e.key.toLowerCase();
  if (key === "z") engine.zoomToSelection();
  else if (key === "c") engine.pickSelectionCluster();
  else return;
  e.preventDefault();
}

export function Galaxy() {
  const hostRef = useRef<HTMLDivElement>(null);
  const glRef = useRef<HTMLCanvasElement>(null);
  const overlayRef = useRef<HTMLCanvasElement>(null);
  const engineRef = useRef<GalaxyEngine | null>(null);
  const [edges, setEdges] = useState<EdgeMode>("off");
  const [labels, setLabels] = useState(true);
  const [tip, setTip] = useState<TooltipAnchor | null>(null);
  const [legend, setLegend] = useState<LegendEntry[]>([]);
  const [stats, setStats] = useState<GalaxyStats | null>(null);
  const snapshot = useAtlas((s) => s.snapshot);
  const setFilters = useAtlas((s) => s.setFilters);
  const showAll = useAtlas((s) => s.showAll);
  const date = usePlayheadDate();

  useEffect(() => {
    const host = hostRef.current;
    const gl = glRef.current;
    const overlay = overlayRef.current;
    if (!host || !gl || !overlay) return;
    const engine = new GalaxyEngine(host, gl, overlay, { tooltip: setTip, legend: setLegend, stats: setStats }, { edges: "off", labels: true });
    engineRef.current = engine;
    const onKey = (e: KeyboardEvent) => onStageKey(engine, e);
    addEventListener("keydown", onKey);
    return () => {
      removeEventListener("keydown", onKey);
      engine.destroy();
      engineRef.current = null;
    };
  }, []);

  useEffect(() => {
    engineRef.current?.setOptions({ edges, labels });
  }, [edges, labels]);

  return (
    <div style={{ position: "absolute", inset: 0, display: "flex", flexDirection: "column" }}>
      <ViewHeader title="Galaxy" summary={stats && snapshot ? summary(stats, snapshot.conversations.length, date) : null}>
        <ColorByControl />
        <SegmentedControl label="Edges" size="sm" value={edges} options={EDGE_OPTIONS} onChange={setEdges} />
        <Chip selected={labels} onClick={() => setLabels(!labels)} title="Cluster labels">
          Labels
        </Chip>
      </ViewHeader>
      <div ref={hostRef} style={{ position: "relative", flex: 1, minHeight: 0, overflow: "hidden" }}>
        <canvas ref={glRef} style={{ position: "absolute", inset: 0, width: "100%", height: "100%" }} />
        <canvas ref={overlayRef} style={{ position: "absolute", inset: 0, pointerEvents: "none" }} />
        <ColorLegend entries={legend} />
        {stats && stats.visible === 0 ? (
          <div style={{ position: "absolute", inset: 0, pointerEvents: "none" }}>
            <Empty stats={stats} date={date} onClear={() => setFilters(NO_FILTERS)} onShowAll={showAll} />
          </div>
        ) : null}
        <StageHint>Shift-drag to lasso, or C to pick the selection's cluster · double-click or Z to zoom to it · Esc clears</StageHint>
      </div>
      <Tooltip at={tip}>{tip && snapshot ? <ConversationTip guid={tip.guid} snapshot={snapshot} /> : null}</Tooltip>
    </div>
  );
}

function summary(s: GalaxyStats, total: number, date: string): string {
  const ghosts = s.existing - s.visible;
  // conversations with too few work turns get no embedding, so no map position
  const unmapped = total - s.points;
  return [
    `${s.visible.toLocaleString()} of ${s.points.toLocaleString()} conversations${ghosts ? ` (${ghosts.toLocaleString()} filtered)` : ""}`,
    ...(unmapped ? [`${unmapped.toLocaleString()} not mapped (too little work to embed)`] : []),
    `up to ${date}`,
  ].join(" · ");
}

/** Why the map is empty at this playhead, and the one action that fixes it. */
function Empty({ stats, date, onClear, onShowAll }: { stats: GalaxyStats; date: string; onClear: () => void; onShowAll: () => void }) {
  const action = (label: string, run: () => void) => (
    <button type="button" className="ca-btn" style={{ pointerEvents: "auto" }} onClick={run}>
      {label}
    </button>
  );
  if (stats.points === 0) {
    return <EmptyState title="Nothing to map in this snapshot">No conversation has enough work turns to embed yet.</EmptyState>;
  }
  if (stats.existing === 0) {
    return (
      <EmptyState title={`No mapped conversation has started by ${date}`} action={action("Show everything", onShowAll)}>
        Play forward, or jump to the end of the data.
      </EmptyState>
    );
  }
  return (
    <EmptyState title="No conversations on the map match these filters" action={action("Clear filters", onClear)}>
      {stats.existing.toLocaleString()} have started by {date}; the filters hide all of them.
    </EmptyState>
  );
}
