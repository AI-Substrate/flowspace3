/**
 * Tag co-occurrence as a reorderable matrix.
 *
 * Why a matrix, not a force graph: co-occurrence among the top tags is dense
 * (most frequent tags meet most others), so a force layout hairballs past a few
 * dozen nodes and cannot show edge weight without clutter. A matrix shows every
 * pair's weight at once, stays legible at 80 tags, and with clustered ordering
 * turns "tags that travel together" into visible blocks on the diagonal.
 *
 * Counts come from the snapshot's tags.cooc when nothing is filtered and the
 * playhead is at the end; otherwise they are recomputed from conversation.tags
 * over the conversations that pass the filters and exist at the playhead.
 */
import * as d3 from "d3";
import { useEffect, useMemo, useRef, useState } from "react";
import { NO_FILTERS, dayOf, filterPredicate, isFiltered } from "../../core/semantics";
import { atlas, useAtlas } from "../../core/store";
import { accent, categorical, css, neutral, withAlpha } from "../../design/palette";
import { EmptyState, SegmentedControl, Tooltip, ViewHeader } from "../../design/primitives";
import { clusterOrder, jaccard, recomputedTagMatrix, snapshotTagMatrix, throughDay } from "./transforms";
import { stageStyle, stepIndex, useSize, useTheme } from "./ui";

type TopN = "30" | "50" | "80";
type CellMode = "count" | "association";
type Order = "clustered" | "frequency";
const TOP_OPTIONS = (["30", "50", "80"] as const).map((v) => ({ value: v, label: v, title: `top ${v} tags by conversations` }));
const CELL_OPTIONS = [
  { value: "count", label: "Count", title: "conversations carrying both tags" },
  { value: "association", label: "Association", title: "Jaccard: both ÷ either; ignores how common each tag is" },
] satisfies { value: CellMode; label: string; title: string }[];
const ORDER_OPTIONS = [
  { value: "clustered", label: "Clustered", title: "average-linkage seriation: tags that travel together sit together" },
  { value: "frequency", label: "Frequency", title: "most common tags first" },
] satisfies { value: Order; label: string; title: string }[];
const LABEL_W = 150;
const LABEL_H = 120;
const PAD = 12;
/** height of the sticky column-label band */
const HEAD = PAD + LABEL_H;
/** room right of the last column for its -60° label to finish */
const LABEL_TAIL = 56;
/** smallest row pitch: fits an 11 px label */
const MIN_ROW = 14;

export function Tags() {
  const snapshot = useAtlas((s) => s.snapshot);
  const filters = useAtlas((s) => s.filters);
  const setFilters = useAtlas((s) => s.setFilters);
  // re-render per simulated day while playing, on every move while scrubbing
  const tick = useAtlas((s) => (s.snapshot && s.time.playing ? dayOf(s.snapshot, s.time.cursorMs) : s.time.cursorMs));
  const theme = useTheme();

  const [topN, setTopN] = useState<TopN>("50");
  const [cell, setCell] = useState<CellMode>("association");
  const [order, setOrder] = useState<Order>("clustered");
  const [hover, setHover] = useState<{ i: number; j: number; at: { x: number; y: number } } | null>(null);
  // keyboard: the row labels are one roving tab stop (Up/Down move, Enter/Space searches the
  // tag); the matrix is another, with a cell cursor (arrows move, Enter/Space searches the row's tag)
  const [roveK, setRoveK] = useState(0);
  const [focusK, setFocusK] = useState<number | null>(null);
  const [cur, setCur] = useState({ r: 0, c: 0 });
  const [gridFocused, setGridFocused] = useState(false);
  const labelRefs = useRef<(SVGGElement | null)[]>([]);
  const [boxRef, size] = useSize<HTMLDivElement>();

  const matrix = useMemo(() => {
    if (!snapshot) return null;
    const cursor = atlas.getState().time.cursorMs;
    return !isFiltered(filters) && cursor >= snapshot.meta.end_ms
      ? snapshotTagMatrix(snapshot, Number(topN))
      : recomputedTagMatrix(snapshot, filterPredicate(snapshot, filters), cursor, Number(topN));
    // `tick` is the cursor's render key: recompute as the playhead moves
  }, [snapshot, filters, topN, tick]);

  const N = matrix?.tags.length ?? 0;
  const assoc = useMemo(() => (matrix ? jaccard(matrix) : new Float64Array()), [matrix]);
  const perm = useMemo(() => (order === "clustered" ? clusterOrder(assoc, N) : d3.range(N)), [assoc, N, order]);
  const values = cell === "count" ? matrix?.cooc : assoc;

  // the matrix fills the stage width; rows share its height but never drop below MIN_ROW
  // (readable labels), past which the matrix scrolls under its sticky column labels
  const mw = Math.max(0, size.width - LABEL_W - LABEL_TAIL - PAD * 2);
  const viewH = Math.max(0, size.height - HEAD - PAD);
  const cw = N > 0 ? mw / N : 0;
  const ch = N > 0 ? Math.max(MIN_ROW, viewH / N) : 0;
  const mh = ch * N;
  const scrollRef = useRef<HTMLDivElement>(null);
  /** scroll row `r` into the part of the scroller below the sticky column labels */
  const reveal = (r: number) => {
    const el = scrollRef.current;
    if (!el) return;
    const top = HEAD + r * ch;
    if (top < el.scrollTop + HEAD) el.scrollTop = top - HEAD;
    else if (top + ch > el.scrollTop + el.clientHeight) el.scrollTop = top + ch - el.clientHeight;
  };

  // cells on canvas; hover never redraws them
  const canvasRef = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const cv = canvasRef.current;
    if (!cv || !matrix || !values || mw <= 0 || mh <= 0) return;
    const dpr = window.devicePixelRatio || 1;
    cv.width = Math.round(mw * dpr);
    cv.height = Math.round(mh * dpr);
    const ctx = cv.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, mw, mh);
    let max = 0;
    for (let i = 0; i < N; i++) for (let j = 0; j < N; j++) if (i !== j) max = Math.max(max, values[i * N + j]);
    const base = categorical(0, theme);
    const diag = css(neutral(0.35, theme));
    const gx = cw > 6 ? 1 : 0;
    const gy = ch > 6 ? 1 : 0;
    perm.forEach((ri, r) =>
      perm.forEach((ci, c) => {
        const v = values[ri * N + ci];
        if (ri === ci) ctx.fillStyle = diag;
        else if (v > 0 && max > 0) ctx.fillStyle = css(withAlpha(base, 0.08 + 0.92 * Math.sqrt(v / max)));
        else return;
        ctx.fillRect(c * cw, r * ch, cw - gx, ch - gy);
      }),
    );
  }, [matrix, values, perm, N, mw, mh, cw, ch, theme]);

  const query = filters.query.trim().toLowerCase();
  const toggleTag = (tag: string) => setFilters({ query: query === tag.toLowerCase() ? "" : tag });

  const onMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const box = e.currentTarget.getBoundingClientRect();
    const c = Math.floor((e.clientX - box.left) / cw);
    const r = Math.floor((e.clientY - box.top) / ch);
    if (r < 0 || c < 0 || r >= N || c >= N) return setHover(null);
    setHover({ i: perm[r], j: perm[c], at: { x: e.clientX, y: e.clientY } });
  };
  const rowOf = useMemo(() => {
    const inv = new Array<number>(N);
    perm.forEach((v, k) => (inv[v] = k));
    return inv;
  }, [perm, N]);

  const rowFont = Math.max(11, Math.min(13, ch - 3));
  const colFont = Math.max(11, Math.min(13, cw - 3));
  const accentCss = css(accent(theme));
  const labelColor = (t: number) =>
    query === matrix?.tags[t].toLowerCase() || focusK === rowOf[t]
      ? accentCss
      : hover && (hover.i === t || hover.j === t)
        ? "var(--ink)"
        : "var(--ink-2)";

  const moveCell = (p: { r: number; c: number }, el: HTMLCanvasElement) => {
    setCur(p);
    reveal(p.r);
    const box = el.getBoundingClientRect();
    setHover({ i: perm[p.r], j: perm[p.c], at: { x: box.left + (p.c + 0.5) * cw, y: box.top + (p.r + 1) * ch } });
  };
  // one wording for the tooltip and the screen-reader live region
  const pairTitle = hover && matrix ? matrix.tags[hover.i] + (hover.i !== hover.j ? ` × ${matrix.tags[hover.j]}` : "") : "";
  const pairDetail =
    hover && matrix
      ? hover.i === hover.j
        ? `${d3.format(",")(matrix.n[hover.i])} conversations`
        : `${d3.format(",")(matrix.cooc[hover.i * N + hover.j])} conversations carry both · association ${d3.format(".2f")(assoc[hover.i * N + hover.j])}`
      : "";

  return (
    <div style={stageStyle}>
      <ViewHeader
        title="Tags"
        summary={
          matrix ? (
            <>
              {N} tags over {d3.format(",")(matrix.conversations)} conversations ·{" "}
              {matrix.source === "snapshot"
                ? "snapshot co-occurrence"
                : `recomputed for the filters, ${snapshot ? throughDay(snapshot, atlas.getState().time.cursorMs) : "at the playhead"}`}{" "}
              · click a tag to search it
            </>
          ) : null
        }
      >
        <SegmentedControl label="Tags" value={topN} options={TOP_OPTIONS} onChange={setTopN} size="sm" />
        <SegmentedControl label="Cell" value={cell} options={CELL_OPTIONS} onChange={setCell} size="sm" />
        <SegmentedControl label="Order" value={order} options={ORDER_OPTIONS} onChange={setOrder} size="sm" />
      </ViewHeader>
      <div ref={boxRef} style={{ position: "relative", flex: 1, minHeight: 0 }}>
        {snapshot && N === 0 && (
          <EmptyState
            title="No tags at the playhead"
            action={
              isFiltered(filters) ? (
                <button type="button" className="ca-btn" onClick={() => setFilters(NO_FILTERS)}>
                  Clear filters
                </button>
              ) : undefined
            }
          >
            No conversation that passes the filters and has started by the playhead carries a tag.
          </EmptyState>
        )}
        {matrix && N > 0 && mw > 0 && mh > 0 && (
          <div
            ref={scrollRef}
            style={{ position: "absolute", inset: 0, overflowY: "auto", overflowX: "hidden" }}
            // a wheel scroll strands a pointer tooltip; keyboard tooltips follow their own reveal()
            onScroll={() => !gridFocused && focusK === null && setHover(null)}
          >
            <div style={{ position: "sticky", top: 0, zIndex: 1, height: HEAD, background: "var(--bg)" }}>
              <svg width={size.width} height={HEAD} aria-hidden="true" style={{ display: "block", pointerEvents: "none" }}>
                <g transform={`translate(${PAD + LABEL_W},${HEAD})`}>
                  {perm.map((t, k) => (
                    <text
                      key={matrix.tags[t]}
                      transform={`translate(${k * cw + cw / 2},-6) rotate(-60)`}
                      dy="0.32em"
                      fontSize={colFont}
                      fill={labelColor(t)}
                    >
                      {matrix.tags[t]}
                    </text>
                  ))}
                </g>
              </svg>
            </div>
            <div style={{ position: "relative", height: mh + PAD }}>
              <canvas
                ref={canvasRef}
                tabIndex={0}
                role="group"
                aria-roledescription="matrix"
                aria-label={`Tag co-occurrence, ${N} by ${N}. Arrow keys move between cells; Enter searches the row's tag.`}
                style={{ position: "absolute", left: PAD + LABEL_W, top: 0, width: mw, height: mh, cursor: "crosshair" }}
                onMouseMove={onMove}
                onMouseLeave={() => setHover(null)}
                onClick={() => hover && toggleTag(matrix.tags[hover.i])}
                onFocus={(e) => {
                  setGridFocused(true);
                  moveCell({ r: Math.min(cur.r, N - 1), c: Math.min(cur.c, N - 1) }, e.currentTarget);
                }}
                onBlur={() => {
                  setGridFocused(false);
                  setHover(null);
                }}
                onKeyDown={(e) => {
                  const p = { r: Math.min(cur.r, N - 1), c: Math.min(cur.c, N - 1) };
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    toggleTag(matrix.tags[perm[p.r]]);
                    return;
                  }
                  const r = stepIndex(e.key, p.r, N, "y");
                  const c = stepIndex(e.key, p.c, N, "x");
                  if (r === null && c === null) return;
                  e.preventDefault();
                  moveCell({ r: r ?? p.r, c: c ?? p.c }, e.currentTarget);
                }}
              />
              <svg width={size.width} height={mh + PAD} style={{ position: "absolute", left: 0, top: 0, pointerEvents: "none" }}>
                <g transform={`translate(${PAD + LABEL_W},0)`}>
                  {hover && (
                    <>
                      <rect x={0} y={rowOf[hover.i] * ch} width={mw} height={ch} fill="none" stroke="var(--line-strong)" />
                      <rect x={rowOf[hover.j] * cw} y={0} width={cw} height={mh} fill="none" stroke="var(--line-strong)" />
                    </>
                  )}
                  {gridFocused && hover && (
                    <rect x={rowOf[hover.j] * cw} y={rowOf[hover.i] * ch} width={cw} height={ch} fill="none" stroke={accentCss} strokeWidth={2} />
                  )}
                  {perm.map((t, k) => (
                    <g
                      key={matrix.tags[t]}
                      ref={(el) => {
                        labelRefs.current[k] = el;
                      }}
                      role="button"
                      tabIndex={k === Math.min(roveK, N - 1) ? 0 : -1}
                      aria-pressed={query === matrix.tags[t].toLowerCase()}
                      aria-label={`Search tag ${matrix.tags[t]} (${d3.format(",")(matrix.n[t])} conversations)`}
                      style={{ pointerEvents: "auto", cursor: "pointer", outline: "none" }}
                      onClick={() => toggleTag(matrix.tags[t])}
                      onFocus={() => {
                        setRoveK(k);
                        setFocusK(k);
                        reveal(k);
                        const box = canvasRef.current?.getBoundingClientRect();
                        if (box) setHover({ i: t, j: t, at: { x: box.left + 4, y: box.top + (k + 1) * ch } });
                      }}
                      onBlur={() => {
                        setFocusK(null);
                        setHover(null);
                      }}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
                          toggleTag(matrix.tags[t]);
                          return;
                        }
                        const next = stepIndex(e.key, k, N);
                        if (next === null) return;
                        e.preventDefault();
                        // reveal() keeps the row clear of the sticky labels; a native focus scroll would not
                        labelRefs.current[next]?.focus({ preventScroll: true });
                      }}
                    >
                      <text
                        x={-6}
                        y={k * ch + ch / 2}
                        dy="0.32em"
                        textAnchor="end"
                        fontSize={rowFont}
                        fill={labelColor(t)}
                        textDecoration={focusK === k ? "underline" : undefined}
                      >
                        {matrix.tags[t]}
                      </text>
                    </g>
                  ))}
                </g>
              </svg>
            </div>
          </div>
        )}
        <Tooltip at={hover ? hover.at : null}>
          {hover && matrix && (
            <>
              <strong>{pairTitle}</strong>
              <div>{pairDetail}</div>
              <div style={{ color: "var(--muted)" }}>click or Enter to search “{matrix.tags[hover.i]}”</div>
            </>
          )}
        </Tooltip>
        <div className="visually-hidden" aria-live="polite">
          {gridFocused || focusK !== null ? `${pairTitle}: ${pairDetail}` : ""}
        </div>
      </div>
    </div>
  );
}
