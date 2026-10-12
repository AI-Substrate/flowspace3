/**
 * Fleet timeline: streamgraph of daily turns stacked by repo / harness /
 * community / kind. It respects filters, follows the playhead imperatively,
 * brushes `time.range`, filters by clicking a stream, and shows insight time
 * ranges as bands.
 */
import * as d3 from "d3";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Insight, InsightIndex, Ms } from "../../contract/types";
import type { SnapshotSlots } from "../../core/colorSlots";
import { loadInsight } from "../../core/data";
import { DAY_MS, NO_FILTERS, dayOf, filterPredicate, fullRange } from "../../core/semantics";
import { atlas, useAtlas } from "../../core/store";
import {
  type RGBA,
  accent,
  categorical,
  communityColor,
  css,
  harnessColor,
  kindColor,
  neutral,
  withAlpha,
} from "../../design/palette";
import { EmptyState, Legend, SegmentedControl, Tooltip, ViewHeader } from "../../design/primitives";
import {
  type GroupBy,
  type StackMode,
  type StreamGroup,
  OTHER_KEY,
  dailyMatrix,
  filterIsActive,
  smoothDaily,
  stackLayers,
} from "./transforms";
import { stageStyle, stepIndex, useSize, useTheme } from "./ui";

const MARGIN = { top: 26, right: 16, bottom: 22, left: 16 };
const STRIP_H = 36;
const STRIP_GAP = 10;
const SMOOTH_WINDOW = 7;
/** "(no repo)"-style streams are hatched so they never read as the "other" bucket */
const MISSING_HATCH = "chronos-timeline-missing";

const GROUP_OPTIONS = [
  { value: "repo", label: "Repo" },
  { value: "harness", label: "Harness" },
  { value: "community", label: "Community" },
  { value: "kind", label: "Kind" },
] satisfies { value: GroupBy; label: string }[];
const SHAPE_OPTIONS = [
  { value: "stream", label: "Stream", title: "centred streamgraph: shape of the work" },
  { value: "stacked", label: "Stacked", title: "zero baseline: read totals off the axis" },
] satisfies { value: StackMode; label: string; title: string }[];
const SMOOTH_OPTIONS = [
  { value: "daily", label: "Daily" },
  { value: "7d", label: "7-day", title: `centred ${SMOOTH_WINDOW}-day moving average` },
] satisfies { value: string; label: string; title?: string }[];

/**
 * Colour a stream the way every other view colours that dimension: sticky slots for
 * repos and communities, fixed palette slots for harnesses and kinds. The "other"
 * bucket is a light neutral; missing values ("(no repo)") a dark one, drawn hatched.
 */
function groupColor(g: StreamGroup, by: GroupBy, slots: SnapshotSlots): RGBA {
  if (g.key === OTHER_KEY) return neutral(0.6);
  if (g.colorKey === null) return neutral(0.28);
  switch (by) {
    case "repo":
      return categorical(slots.repo.get(g.colorKey) ?? 0);
    case "harness":
      return harnessColor(g.colorKey);
    case "community":
      return communityColor(slots.community.get(g.colorKey) ?? null);
    case "kind":
      return kindColor(g.colorKey as "session" | "subagent");
  }
}

const insightCache = new Map<string, Promise<Insight | null>>();
function cachedInsight(id: string): Promise<Insight | null> {
  let p = insightCache.get(id);
  if (!p) {
    p = loadInsight(id).catch(() => null);
    insightCache.set(id, p);
  }
  return p;
}

/** Every insight with a time range; refetches nothing already loaded. */
function useTimedInsights(index: InsightIndex | null): Insight[] {
  const [list, setList] = useState<Insight[]>([]);
  useEffect(() => {
    if (!index) return;
    let live = true;
    void Promise.all(index.insights.map((e) => cachedInsight(e.id))).then((docs) => {
      if (live) setList(docs.filter((d): d is Insight => !!d?.refs.time_range));
    });
    return () => {
      live = false;
    };
  }, [index]);
  return list;
}

export function Timeline() {
  const snapshot = useAtlas((s) => s.snapshot);
  const filters = useAtlas((s) => s.filters);
  const setFilters = useAtlas((s) => s.setFilters);
  const setTime = useAtlas((s) => s.setTime);
  const range = useAtlas((s) => s.time.range);
  const insightIndex = useAtlas((s) => s.insights);
  const activeInsight = useAtlas((s) => s.activeInsight);
  const setActiveInsight = useAtlas((s) => s.setActiveInsight);
  const insights = useTimedInsights(insightIndex);
  const theme = useTheme();

  const [groupBy, setGroupBy] = useState<GroupBy>("repo");
  const [mode, setMode] = useState<StackMode>("stream");
  const [smooth, setSmooth] = useState("7d");
  const [hover, setHover] = useState<{ g: number; day: number; at: { x: number; y: number } } | null>(null);
  const [boxRef, size] = useSize<HTMLDivElement>();
  // keyboard: the streams are one roving tab stop (Up/Down move, Enter/Space filter);
  // focus shows the hover readout at the playhead's day
  const [roveG, setRoveG] = useState(0);
  const [focusG, setFocusG] = useState<number | null>(null);
  const [focusIns, setFocusIns] = useState<string | null>(null);
  const pathRefs = useRef<(SVGPathElement | null)[]>([]);

  const raw = useMemo(
    () => (snapshot ? dailyMatrix(snapshot, filterPredicate(snapshot, filters), groupBy) : null),
    [snapshot, filters, groupBy],
  );
  const matrix = useMemo(() => (raw && smooth === "7d" ? smoothDaily(raw, SMOOTH_WINDOW) : raw), [raw, smooth]);
  const layers = useMemo(() => (matrix ? stackLayers(matrix, mode) : []), [matrix, mode]);
  // theme is a dep: palette functions read the live theme
  const colors = useMemo(
    () => (snapshot && matrix ? matrix.groups.map((g) => groupColor(g, groupBy, snapshot.slots)) : []),
    [snapshot, matrix, groupBy, theme],
  );

  const w = Math.max(0, size.width - MARGIN.left - MARGIN.right);
  const chartH = Math.max(0, size.height - MARGIN.top - MARGIN.bottom - STRIP_H - STRIP_GAP);
  const day0 = snapshot?.meta.day0_ms ?? 0;
  const days = snapshot?.meta.days ?? 1;
  // the axis spans whole days; the playable range ends at the newest turn
  const full: [Ms, Ms] = snapshot ? fullRange(snapshot.meta) : [day0, day0];
  const endMs = full[1];

  const x = useMemo(() => d3.scaleUtc().domain([day0, day0 + days * DAY_MS]).range([0, w]), [day0, days, w]);
  const dayX = (d: number) => x(day0 + (d + 0.5) * DAY_MS);

  const y = useMemo(() => {
    const lo = d3.min(layers, (l) => d3.min(l, (p) => p[0])) ?? 0;
    const hi = d3.max(layers, (l) => d3.max(l, (p) => p[1])) ?? 1;
    return d3
      .scaleLinear()
      .domain([lo, hi > lo ? hi : lo + 1])
      .range([chartH, 0]);
  }, [layers, chartH]);

  const area = d3
    .area<d3.SeriesPoint<number>>()
    .x((_, d) => dayX(d))
    .y0((p) => y(p[0]))
    .y1((p) => y(p[1]))
    .curve(mode === "stream" ? d3.curveBasis : d3.curveMonotoneX);

  // brush strip: total (filtered) turns per day as a context sparkline
  const totals = useMemo(() => {
    if (!raw) return [];
    const G = raw.groups.length;
    return d3.range(raw.days).map((d) => d3.sum(d3.range(G), (g) => raw.values[d * G + g]));
  }, [raw]);
  const stripY = d3
    .scaleLinear()
    .domain([0, d3.max(totals) || 1])
    .range([STRIP_H, 2]);
  const stripArea = d3
    .area<number>()
    .x((_, d) => dayX(d))
    .y0(STRIP_H)
    .y1((v) => stripY(v))
    .curve(d3.curveMonotoneX);

  // playhead: imperative, never re-renders React per frame
  const playRef = useRef<SVGLineElement>(null);
  const stripPlayRef = useRef<SVGLineElement>(null);
  const futureRef = useRef<SVGRectElement>(null);
  useEffect(
    () =>
      atlas.subscribe(
        (s) => s.time.cursorMs,
        (cursor) => {
          const px = Math.max(0, Math.min(w, x(cursor)));
          for (const el of [playRef.current, stripPlayRef.current]) {
            el?.setAttribute("x1", String(px));
            el?.setAttribute("x2", String(px));
          }
          futureRef.current?.setAttribute("x", String(px));
          futureRef.current?.setAttribute("width", String(Math.max(0, w - px)));
        },
        { fireImmediately: true },
      ),
    [x, w],
  );

  // brush ⇄ time.range
  const brushRef = useRef<SVGGElement>(null);
  useEffect(() => {
    const g = brushRef.current;
    if (!g || w <= 0) return;
    const snapDay = (px: number) => day0 + Math.round((x.invert(px).getTime() - day0) / DAY_MS) * DAY_MS;
    const sel = d3.select(g);
    const brush = d3
      .brushX()
      .extent([
        [0, 0],
        [w, STRIP_H],
      ])
      .on("end", (ev: d3.D3BrushEvent<unknown>) => {
        if (!ev.sourceEvent) return; // programmatic move (sync below)
        if (!ev.selection) {
          setTime({ range: [day0, endMs] });
          return;
        }
        const [a, b] = ev.selection as [number, number];
        const start = Math.min(snapDay(a), endMs - 1);
        const end = Math.min(endMs, Math.max(start + DAY_MS, snapDay(b)));
        const cursor = atlas.getState().time.cursorMs;
        setTime({ range: [start, end], cursorMs: Math.min(end, Math.max(start, cursor)) });
      });
    sel.call(brush);
    const unsub = atlas.subscribe(
      (s) => s.time.range,
      ([a, b]) => {
        const isFull = a <= day0 && b >= endMs;
        sel.call(brush.move, isFull ? null : [x(a), x(b)]);
      },
      { fireImmediately: true, equalityFn: (p, q) => p[0] === q[0] && p[1] === q[1] },
    );
    return () => {
      unsub();
      sel.on(".brush", null);
    };
  }, [x, w, day0, endMs, setTime]);

  const toggleGroup = (g: StreamGroup) => {
    if (!g.filter) return;
    if (filterIsActive(filters, g.filter)) {
      setFilters(Object.fromEntries(Object.keys(g.filter).map((k) => [k, NO_FILTERS[k as keyof typeof NO_FILTERS]])));
    } else setFilters(g.filter);
  };

  const onMove = (e: React.MouseEvent<SVGPathElement>, g: number) => {
    const box = boxRef.current?.getBoundingClientRect();
    if (!box) return;
    const px = e.clientX - box.left - MARGIN.left;
    const day = Math.max(0, Math.min(days - 1, Math.floor((x.invert(px).getTime() - day0) / DAY_MS)));
    setHover({ g, day, at: { x: e.clientX, y: e.clientY } });
  };
  const onStreamFocus = (g: number) => {
    setRoveG(g);
    setFocusG(g);
    const box = boxRef.current?.getBoundingClientRect();
    if (!box || !snapshot) return;
    const day = Math.max(0, Math.min(days - 1, dayOf(snapshot, atlas.getState().time.cursorMs)));
    const p = layers[g]?.[day];
    if (p) setHover({ g, day, at: { x: box.left + MARGIN.left + dayX(day), y: box.top + MARGIN.top + y((p[0] + p[1]) / 2) } });
  };

  const fmtDay = d3.utcFormat("%-d %b %Y");
  const ticks = x.ticks(Math.max(2, Math.floor(w / 90)));
  const tickFmt = d3.utcFormat("%-d %b");
  const hoverGroup = hover && matrix ? matrix.groups[hover.g] : null;
  const hoverVal = hover && matrix ? matrix.values[hover.day * matrix.groups.length + hover.g] : 0;
  const isFullRange = range[0] <= full[0] && range[1] >= full[1];
  const totalTurns = raw ? d3.sum(raw.groups, (g) => g.total) : 0;
  const accentCss = css(accent());

  return (
    <div style={stageStyle}>
      <ViewHeader
        title="Timeline"
        summary={
          <>
            {d3.format(",")(totalTurns)} turns ·{" "}
            {isFullRange
              ? "playing the whole snapshot"
              : `playing ${fmtDay(new Date(range[0]))} – ${fmtDay(new Date(Math.min(range[1], endMs)))}`}{" "}
            ·
            brush the strip to set the range
          </>
        }
      >
        <SegmentedControl label="Stack by" value={groupBy} options={GROUP_OPTIONS} onChange={setGroupBy} size="sm" />
        <SegmentedControl label="Shape" value={mode} options={SHAPE_OPTIONS} onChange={setMode} size="sm" />
        <SegmentedControl label="Smoothing" value={smooth} options={SMOOTH_OPTIONS} onChange={setSmooth} size="sm" />
      </ViewHeader>
      <div style={{ display: "flex", flex: 1, minHeight: 0 }}>
        <div ref={boxRef} style={{ position: "relative", flex: 1, minWidth: 0, minHeight: 0 }}>
          {!snapshot ? null : !matrix || matrix.groups.length === 0 ? (
            <EmptyState
              title="No turns match these filters"
              action={
                <button type="button" className="ca-btn" onClick={() => setFilters(NO_FILTERS)}>
                  Clear filters
                </button>
              }
            >
              The timeline stacks the daily turns of conversations that pass the filters.
            </EmptyState>
          ) : (
            w > 0 &&
            chartH > 0 && (
              <svg width={size.width} height={size.height} style={{ display: "block" }}>
                <defs>
                  <pattern id={MISSING_HATCH} width={6} height={6} patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
                    <rect width={6} height={6} fill={css(neutral(0.28))} />
                    <line x1={0} y1={0} x2={0} y2={6} stroke={css(neutral(0.55))} strokeWidth={2} />
                  </pattern>
                </defs>
                <g transform={`translate(${MARGIN.left},${MARGIN.top})`}>
                  {insights.map((ins) => {
                    const tr = ins.refs.time_range as { start_ms: Ms; end_ms: Ms };
                    const x0 = Math.max(0, x(tr.start_ms));
                    const x1 = Math.min(w, x(tr.end_ms));
                    if (x1 < 0 || x0 > w) return null;
                    const bw = Math.max(2, x1 - x0);
                    const on = ins.id === activeInsight;
                    const c = kindColor(ins.kind ?? "finding");
                    const fullTitle = `${ins.kind ?? "insight"}: ${ins.title}`;
                    const fit = Math.floor((bw - 8) / 6.5);
                    const label =
                      bw <= 40 ? "✦" : ins.title.length <= fit ? ins.title : `${ins.title.slice(0, fit - 1).trimEnd()}…`;
                    return (
                      <g
                        key={ins.id}
                        role="button"
                        tabIndex={0}
                        aria-pressed={on}
                        aria-label={`Insight · ${fullTitle}`}
                        style={{ cursor: "pointer", outline: "none" }}
                        onClick={() => setActiveInsight(on ? null : ins.id)}
                        onKeyDown={(e) => {
                          if (e.key !== "Enter" && e.key !== " ") return;
                          e.preventDefault();
                          setActiveInsight(on ? null : ins.id);
                        }}
                        onFocus={() => setFocusIns(ins.id)}
                        onBlur={() => setFocusIns(null)}
                      >
                        <title>{fullTitle}</title>
                        <rect
                          x={x0}
                          y={-MARGIN.top + 4}
                          width={bw}
                          height={chartH + MARGIN.top - 4}
                          fill={css(withAlpha(c, on ? 0.2 : 0.07))}
                          stroke={on ? accentCss : css(withAlpha(c, 0.35))}
                          style={{ pointerEvents: "none" }}
                        />
                        <rect
                          x={x0}
                          y={-MARGIN.top + 4}
                          width={bw}
                          height={16}
                          fill={on ? accentCss : css(withAlpha(c, 0.45))}
                          stroke={focusIns === ins.id ? "var(--ink)" : undefined}
                          strokeWidth={2}
                        >
                          <title>{fullTitle}</title>
                        </rect>
                        <text x={x0 + 4} y={-MARGIN.top + 16} fill={on ? "var(--on-accent)" : "var(--ink)"} fontSize="var(--fs-xs)">
                          <title>{fullTitle}</title>
                          {label}
                        </text>
                      </g>
                    );
                  })}
                  {layers.map((layer, g) => {
                    const grp = matrix.groups[g];
                    const dim = hover !== null && hover.g !== g;
                    return (
                      <path
                        key={grp.key}
                        ref={(el) => {
                          pathRefs.current[g] = el;
                        }}
                        role="button"
                        tabIndex={g === Math.min(roveG, layers.length - 1) ? 0 : -1}
                        aria-label={`${grp.label}: ${d3.format(",")(Math.round(grp.total))} turns${grp.filter ? "" : " (cannot filter)"}`}
                        aria-pressed={grp.filter ? filterIsActive(filters, grp.filter) : undefined}
                        aria-disabled={grp.filter ? undefined : true}
                        d={area(layer) ?? ""}
                        fill={grp.colorKey === null && grp.key !== OTHER_KEY ? `url(#${MISSING_HATCH})` : css(colors[g])}
                        fillOpacity={dim ? 0.45 : 0.92}
                        stroke="var(--bg)"
                        strokeWidth={0.5}
                        style={{
                          cursor: grp.filter ? "pointer" : "default",
                          outline: "none",
                          transition: "d var(--dur-base) var(--ease-out), fill-opacity var(--dur-fast)",
                        }}
                        onMouseMove={(e) => onMove(e, g)}
                        onMouseLeave={() => setHover(null)}
                        onClick={() => toggleGroup(grp)}
                        onFocus={() => onStreamFocus(g)}
                        onBlur={() => {
                          setFocusG(null);
                          setHover(null);
                        }}
                        onKeyDown={(e) => {
                          if (e.key === "Enter" || e.key === " ") {
                            e.preventDefault();
                            toggleGroup(grp);
                            return;
                          }
                          const next = stepIndex(e.key, g, layers.length);
                          if (next === null) return;
                          e.preventDefault();
                          pathRefs.current[next]?.focus();
                        }}
                      />
                    );
                  })}
                  {focusG !== null && layers[focusG] && (
                    // focus ring on top of the neighbouring streams
                    <path d={area(layers[focusG]) ?? ""} fill="none" stroke={accentCss} strokeWidth={2} style={{ pointerEvents: "none" }} />
                  )}
                  <rect ref={futureRef} y={0} height={chartH} fill="var(--bg)" fillOpacity={0.6} style={{ pointerEvents: "none" }} />
                  <line ref={playRef} y1={-6} y2={chartH} stroke={accentCss} strokeWidth={1.5} style={{ pointerEvents: "none" }} />
                  <g transform={`translate(0,${chartH})`}>
                    <line x1={0} x2={w} stroke="var(--line)" />
                    {ticks.map((t) => (
                      <g key={t.getTime()} transform={`translate(${x(t)},0)`}>
                        <line y2={4} stroke="var(--line-strong)" />
                        <text y={16} textAnchor="middle" fill="var(--muted)" fontSize="var(--fs-2xs)">
                          {tickFmt(t)}
                        </text>
                      </g>
                    ))}
                  </g>
                  <g transform={`translate(0,${chartH + MARGIN.bottom + STRIP_GAP})`}>
                    <rect width={w} height={STRIP_H} fill="var(--panel)" />
                    <path d={stripArea(totals) ?? ""} fill="var(--faint)" />
                    <line ref={stripPlayRef} y1={0} y2={STRIP_H} stroke={accentCss} strokeWidth={1.5} />
                    <g ref={brushRef} />
                  </g>
                </g>
              </svg>
            )
          )}
          <Tooltip at={hover && hoverGroup ? hover.at : null}>
            {hover && hoverGroup && (
              <>
                <strong>{hoverGroup.label}</strong>
                <div style={{ color: "var(--muted)" }}>{fmtDay(new Date(day0 + hover.day * DAY_MS))}</div>
                <div>
                  {d3.format(",.1~f")(hoverVal)} turns{smooth === "7d" ? ` (${SMOOTH_WINDOW}-day avg)` : ""}
                </div>
                {hoverGroup.filter && <div style={{ color: "var(--muted)" }}>click or Enter to filter</div>}
              </>
            )}
          </Tooltip>
        </div>
        {snapshot && matrix && matrix.groups.length > 0 && (
          // beside the plot, never over it: the stream stays fully visible
          <aside style={{ flex: "none", maxHeight: "100%", overflowY: "auto", paddingTop: MARGIN.top - 4 }}>
            <Legend
              title={GROUP_OPTIONS.find((o) => o.value === groupBy)?.label ?? groupBy}
              shape="square"
              items={matrix.groups.map((g, k) => ({ key: g.key, label: g.label, color: colors[k], count: Math.round(g.total) }))}
              onToggle={(key) => {
                const g = matrix.groups.find((q) => q.key === key);
                if (g) toggleGroup(g);
              }}
            />
          </aside>
        )}
      </div>
    </div>
  );
}
