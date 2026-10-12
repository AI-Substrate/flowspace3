/**
 * Seat network view: who talks to whom over pij, up to the playhead. Seats are
 * nodes, peer messages are directed links; today's messages fire as pulses.
 * Click a seat to filter every view to it. SeatsEngine owns sigma and the store
 * subscriptions; React renders only the chrome.
 */
import { useEffect, useRef, useState } from "react";
import type { LoadedSnapshot } from "../../../core/data";
import { useAtlas } from "../../../core/store";
import { Chip, Legend, Tooltip, ViewHeader } from "../../../design/primitives";
import type { LegendEntry } from "../lib/coloring";
import { ConversationTip, StageHint, usePlayheadDate } from "../shared";
import { type SeatsAnchor, SeatsEngine, type SeatsStats } from "./engine";

export function Seats() {
  const hostRef = useRef<HTMLDivElement>(null);
  const overlayRef = useRef<HTMLCanvasElement>(null);
  const engineRef = useRef<SeatsEngine | null>(null);
  const [trees, setTrees] = useState(false);
  const [tip, setTip] = useState<SeatsAnchor | null>(null);
  const [legend, setLegend] = useState<LegendEntry[]>([]);
  const [stats, setStats] = useState<SeatsStats | null>(null);
  const snapshot = useAtlas((s) => s.snapshot);
  const date = usePlayheadDate();
  const harnessFilter = useAtlas((s) => s.filters.harnesses);
  const setFilters = useAtlas((s) => s.setFilters);

  useEffect(() => {
    const host = hostRef.current;
    const overlay = overlayRef.current;
    if (!host || !overlay) return;
    const engine = new SeatsEngine(host, overlay, { tooltip: setTip, legend: setLegend, stats: setStats }, { trees: false });
    engineRef.current = engine;
    return () => {
      engine.destroy();
      engineRef.current = null;
    };
  }, []);

  useEffect(() => {
    engineRef.current?.setOptions({ trees });
  }, [trees]);

  const summary = stats ? seatsSummary(stats, date) : null;
  const toggleHarness = (h: string) =>
    setFilters({ harnesses: harnessFilter.includes(h) ? harnessFilter.filter((x) => x !== h) : [...harnessFilter, h] });

  return (
    <div style={{ position: "absolute", inset: 0, display: "flex", flexDirection: "column" }}>
      <ViewHeader title="Seat network" summary={summary}>
        <Chip selected={trees} onClick={() => setTrees(!trees)} title="Show subagent parent→child trees under their seats">
          Subagent trees
        </Chip>
        <button type="button" className="ca-btn" data-variant="ghost" onClick={() => engineRef.current?.relayout()}>
          Re-run layout
        </button>
      </ViewHeader>
      <div style={{ position: "relative", flex: 1, minHeight: 0, overflow: "hidden" }}>
        <div ref={hostRef} style={{ position: "absolute", inset: 0 }} />
        <canvas ref={overlayRef} style={{ position: "absolute", inset: 0, width: "100%", height: "100%", pointerEvents: "none" }} />
        {legend.length ? (
          <div style={{ position: "absolute", left: "var(--sp-6)", bottom: "var(--sp-6)", zIndex: "var(--z-stage-overlay)" }}>
            <Legend
              title="Harness"
              items={legend.map((e) => ({
                key: e.key ?? e.label,
                label: e.label,
                color: e.color,
                count: e.count,
                active: harnessFilter.length === 0 || (e.key !== null && harnessFilter.includes(e.key)),
                // a harness filter can only name a harness
                fixed: e.key === null,
              }))}
              onToggle={toggleHarness}
            />
          </div>
        ) : null}
        <StageHint>Click a seat to filter every view to it · pulses are today's messages</StageHint>
      </div>
      <Tooltip at={tip}>
        {tip && snapshot ? tip.kind === "seat" ? <SeatTip seat={tip.seat} snapshot={snapshot} /> : <ConversationTip guid={tip.guid} snapshot={snapshot} /> : null}
      </Tooltip>
    </div>
  );
}

/**
 * Header line: drawn messages (at least one seat known; an unknown end lands on
 * the "unknown seat" node) out of every message so far, how many of them know
 * both seats, and the undrawn rest.
 */
function seatsSummary(s: SeatsStats, date: string): string {
  const total = s.messages + s.unattributed + s.selfSent;
  const fmt = (n: number) => n.toLocaleString();
  return [
    `${fmt(s.seats)} seats · ${fmt(s.links)} links`,
    `${fmt(s.messages)} of ${fmt(total)} messages drawn${s.firing ? `, ${fmt(s.firing)} firing today` : ""}`,
    `${fmt(s.known)} with both seats known`,
    ...(s.unattributed ? [`${fmt(s.unattributed)} with neither seat known (not drawn)`] : []),
    ...(s.selfSent ? [`${fmt(s.selfSent)} to the sender's own seat (not drawn)`] : []),
    `up to ${date}`,
  ].join(" · ");
}

function SeatTip({ seat, snapshot }: { seat: number; snapshot: LoadedSnapshot }) {
  const s = snapshot.meta.seats[seat];
  const harness = s.harness !== null ? snapshot.meta.harnesses[s.harness] : "unknown harness";
  return (
    <>
      <div className="mono" style={{ fontWeight: "var(--fw-medium)" }}>
        {s.id}
      </div>
      <div style={{ color: "var(--muted)" }}>
        {harness} · {s.conversations.length.toLocaleString()} conversations in this snapshot
      </div>
      <div style={{ color: "var(--muted)" }}>Click to filter to this seat</div>
    </>
  );
}
