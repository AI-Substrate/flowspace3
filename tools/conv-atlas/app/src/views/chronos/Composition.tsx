/**
 * Composition: what each conversation is made of. Per conversation, a class
 * bar (work / tool_call / tool_result / retrieval) and an origin bar (human /
 * peer / agent), the autonomy index (human turns per 100 tool calls), sorting, and small multiples by repo or
 * harness. Bars grow with the playhead (turns so far); the mix inside them is
 * the conversation's final mix (the view layer has no per-day class counts).
 */
import * as d3 from "d3";
import { type CSSProperties, type KeyboardEvent, useId, useMemo, useState } from "react";
import { NO_FILTERS, dayOf, filterPredicate } from "../../core/semantics";
import { atlas, useAtlas } from "../../core/store";
import { css, roleColor, turnClassColor } from "../../design/palette";
import { EmptyState, Legend, SegmentedControl, ViewHeader } from "../../design/primitives";
import {
  CLASS_KEYS,
  COMPOSITION_SORTS,
  type CompositionRow,
  type CompositionSort,
  ORIGIN_KEYS,
  compositionRows,
  facetRows,
  sortRows,
  throughDay,
} from "./transforms";
import { stageStyle, stepIndex, useSize, useTheme } from "./ui";

type FacetBy = "none" | "repo" | "harness";
type Length = "share" | "absolute";
const SORT_OPTIONS = COMPOSITION_SORTS.map((s) => ({
  value: s,
  label: s[0].toUpperCase() + s.slice(1),
  title: s === "autonomy" ? "most autonomous (fewest human turns per tool call) first" : undefined,
}));
const FACET_OPTIONS = [
  { value: "none", label: "All" },
  { value: "repo", label: "By repo" },
  { value: "harness", label: "By harness" },
] satisfies { value: FacetBy; label: string }[];
const LENGTH_OPTIONS = [
  { value: "share", label: "Share", title: "every bar full width: compare mixes" },
  { value: "absolute", label: "Absolute", title: "bar length = turns so far: compare sizes" },
] satisfies { value: Length; label: string; title: string }[];
const ROW_H = 20;
const FACET_ROWS = 10;
const COLS = "minmax(140px, 1.3fr) 84px 72px minmax(120px, 2fr) minmax(90px, 1.2fr)";

const fmtInt = d3.format(",");
const fmtAut = (a: number | null) => (a === null ? "—" : a < 10 ? a.toFixed(1) : Math.round(a).toString());

function Bar({ parts, colors, total, scale, keys }: { parts: number[]; colors: string[]; total: number; scale: number; keys: readonly string[] }) {
  const title = keys.map((k, j) => `${k} ${total > 0 ? Math.round((parts[j] / total) * 100) : 0}%`).join(" · ");
  return (
    <div title={title} style={{ height: 10, width: `${Math.max(0, Math.min(1, scale)) * 100}%`, display: "flex", background: "var(--line)" }}>
      {total > 0 && parts.map((v, j) => <div key={keys[j]} style={{ width: `${(v / total) * 100}%`, background: colors[j] }} />)}
    </div>
  );
}

interface RowProps {
  r: CompositionRow;
  id: string;
  label: string;
  guid: string;
  max: number;
  length: Length;
  selected: boolean;
  /** the keyboard cursor is on this row and the list has focus */
  focused: boolean;
  onPick: () => void;
  colors: { cls: string[]; origin: string[] };
  style?: CSSProperties;
}

function Row({ r, id, label, guid, max, length, selected, focused, onPick, colors, style }: RowProps) {
  const scale = length === "share" ? (r.turnsNow > 0 ? 1 : 0) : r.turnsNow / max;
  return (
    <div
      id={id}
      role="option"
      aria-selected={selected}
      aria-label={`${label}: ${fmtInt(r.turnsNow)} turns so far, autonomy ${fmtAut(r.autonomy)} human per 100 tool calls`}
      onClick={() => {
        onPick();
        atlas.getState().select(guid);
      }}
      onMouseEnter={() => atlas.getState().hover(guid)}
      onMouseLeave={() => atlas.getState().hover(null)}
      style={{
        display: "grid",
        gridTemplateColumns: COLS,
        alignItems: "center",
        gap: 8,
        height: ROW_H,
        padding: "0 12px",
        cursor: "pointer",
        background: selected ? "var(--accent-soft)" : undefined,
        boxShadow: selected ? "inset 2px 0 0 var(--accent)" : undefined,
        outline: focused ? "2px solid var(--accent)" : undefined,
        outlineOffset: -2,
        ...style,
      }}
    >
      <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{label}</span>
      <span style={{ textAlign: "right" }}>{fmtInt(r.turnsNow)}</span>
      <span style={{ textAlign: "right", color: "var(--muted)" }}>{fmtAut(r.autonomy)}</span>
      <Bar parts={r.byClass} colors={colors.cls} total={r.turnsNow} scale={scale} keys={CLASS_KEYS} />
      <Bar parts={r.byOrigin} colors={colors.origin} total={r.turnsNow} scale={scale} keys={ORIGIN_KEYS} />
    </div>
  );
}

function Header() {
  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: COLS,
        gap: 8,
        padding: "4px 12px",
        color: "var(--muted)",
        fontSize: "var(--fs-xs)",
        borderBottom: "1px solid var(--line)",
      }}
    >
      <span>Conversation</span>
      <span style={{ textAlign: "right" }}>Turns so far</span>
      <span style={{ textAlign: "right" }} title="human turns per 100 tool_call turns; lower = more autonomous. Counts tool calls, not agent turns, so harnesses compare">
        Autonomy
      </span>
      <span>Class · final mix</span>
      <span>Origin · final mix</span>
    </div>
  );
}

export function Composition() {
  const snapshot = useAtlas((s) => s.snapshot);
  const filters = useAtlas((s) => s.filters);
  const selected = useAtlas((s) => s.selected);
  const setFilters = useAtlas((s) => s.setFilters);
  // re-render per simulated day while playing, on every move while scrubbing
  const tick = useAtlas((s) => (s.snapshot && s.time.playing ? dayOf(s.snapshot, s.time.cursorMs) : s.time.cursorMs));
  const theme = useTheme();

  const [sort, setSort] = useState<CompositionSort>("turns");
  const [facet, setFacet] = useState<FacetBy>("none");
  const [length, setLength] = useState<Length>("share");
  const [scrollTop, setScrollTop] = useState(0);
  const [listRef, listSize] = useSize<HTMLDivElement>();

  const rows = useMemo(() => {
    if (!snapshot) return [];
    const cursor = atlas.getState().time.cursorMs;
    return sortRows(compositionRows(snapshot, filterPredicate(snapshot, filters), cursor), snapshot, sort);
    // `tick` is the cursor's render key: recompute as the playhead moves
  }, [snapshot, filters, sort, tick]);
  const facets = useMemo(() => (snapshot && facet !== "none" ? facetRows(rows, snapshot, facet) : []), [rows, snapshot, facet]);
  // Origin uses roleColor so it matches the river. `theme` is a dep: palette
  // functions read the live theme.
  const rgba = useMemo(
    () => ({
      cls: CLASS_KEYS.map((k) => turnClassColor(k, theme)),
      origin: [roleColor("human", "human", theme), roleColor("agent", "peer", theme), roleColor("agent", "system", theme)],
    }),
    [theme],
  );
  const colors = useMemo(() => ({ cls: rgba.cls.map(css), origin: rgba.origin.map(css) }), [rgba]);
  const max = d3.max(rows, (r) => r.turnsNow) || 1;

  const first = Math.max(0, Math.floor(scrollTop / ROW_H) - 5);
  const last = Math.min(rows.length, first + Math.ceil(listSize.height / ROW_H) + 10);

  // Keyboard: the list is one tab stop (a listbox); Up/Down/PageUp/PageDown/Home/End move a
  // cursor (aria-activedescendant, since virtualisation unmounts off-screen rows), Enter or
  // Space selects. The cursor follows the conversation, not the position, as rows re-sort.
  const idBase = useId();
  const [cursorI, setCursorI] = useState<number | null>(null);
  const [listFocused, setListFocused] = useState(false);
  const order = useMemo(() => (facet === "none" ? rows : facets.flatMap((f) => f.rows.slice(0, FACET_ROWS))), [rows, facets, facet]);
  const at = Math.max(0, order.findIndex((r) => r.i === cursorI));
  const cursorRow = order[at];
  const rowId = (i: number) => `${idBase}-row-${i}`;
  const guidOf = (r: CompositionRow) => snapshot?.conversations[r.i].guid ?? "";
  const onListKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (!cursorRow) return;
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      atlas.getState().select(guidOf(cursorRow));
      return;
    }
    const next = stepIndex(e.key, at, order.length);
    if (next === null) return;
    e.preventDefault();
    setCursorI(order[next].i);
    atlas.getState().hover(guidOf(order[next]));
    // keep it in view: facet rows are all mounted; flat rows are virtualised, so compute
    const el = e.currentTarget;
    if (facet !== "none") return document.getElementById(rowId(order[next].i))?.scrollIntoView({ block: "nearest" });
    const top = next * ROW_H;
    if (top < el.scrollTop) el.scrollTop = top;
    else if (top + ROW_H > el.scrollTop + el.clientHeight) el.scrollTop = top + ROW_H - el.clientHeight;
  };

  const rowProps = (r: CompositionRow, rowMax: number) => {
    const c = snapshot?.conversations[r.i];
    return {
      r,
      id: rowId(r.i),
      label: c?.label ?? "",
      guid: c?.guid ?? "",
      max: rowMax,
      length,
      selected: c?.guid === selected,
      focused: listFocused && r === cursorRow,
      onPick: () => setCursorI(r.i),
      colors,
    };
  };

  const fleet = snapshot ? facetRows(rows, snapshot, "all")[0] : undefined;
  const through = snapshot ? throughDay(snapshot, atlas.getState().time.cursorMs) : "";

  return (
    <div style={stageStyle}>
      <ViewHeader
        title="Composition"
        summary={`${fmtInt(rows.length)} conversations ${through} · fleet autonomy ${fmtAut(fleet?.autonomy ?? null)} human per 100 tool calls`}
      >
        <SegmentedControl label="Sort" value={sort} options={SORT_OPTIONS} onChange={setSort} size="sm" />
        <SegmentedControl label="Facet" value={facet} options={FACET_OPTIONS} onChange={setFacet} size="sm" />
        <SegmentedControl label="Length" value={length} options={LENGTH_OPTIONS} onChange={setLength} size="sm" />
      </ViewHeader>
      <div style={{ display: "flex", flexWrap: "wrap", alignItems: "center", gap: "4px 24px", padding: "4px 12px" }}>
        <Legend title="Class" shape="square" items={CLASS_KEYS.map((k, j) => ({ key: k, label: k, color: rgba.cls[j] }))} />
        <Legend title="Origin" shape="square" items={ORIGIN_KEYS.map((k, j) => ({ key: k, label: k, color: rgba.origin[j] }))} />
        <span style={{ color: "var(--muted)", fontSize: "var(--fs-xs)" }}>bar length: turns so far · mix: each conversation's final mix</span>
      </div>
      {facet === "none" && <Header />}
      <div
        ref={listRef}
        role="listbox"
        aria-label="Conversations"
        tabIndex={0}
        aria-activedescendant={listFocused && cursorRow ? rowId(cursorRow.i) : undefined}
        onKeyDown={onListKey}
        onFocus={(e) => e.target === e.currentTarget && setListFocused(true)}
        onBlur={(e) => {
          if (e.target !== e.currentTarget) return;
          setListFocused(false);
          atlas.getState().hover(null);
        }}
        onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
        style={{ position: "relative", flex: 1, minHeight: 0, overflowY: "auto" }}
      >
        {snapshot && rows.length === 0 && (
          <EmptyState
            title="No conversation matches at the playhead"
            action={
              <button type="button" className="ca-btn" onClick={() => setFilters(NO_FILTERS)}>
                Clear filters
              </button>
            }
          >
            Composition lists conversations that pass the filters and have started by the playhead.
          </EmptyState>
        )}
        {facet === "none" ? (
          <div style={{ height: rows.length * ROW_H, position: "relative" }}>
            {rows.slice(first, last).map((r, k) => (
              <Row key={r.i} {...rowProps(r, max)} style={{ position: "absolute", top: (first + k) * ROW_H, left: 0, right: 0 }} />
            ))}
          </div>
        ) : (
          <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(560px, 1fr))", gap: 12, padding: 12 }}>
            {facets.map((f) => {
              const fmax = d3.max(f.rows, (r) => r.turnsNow) || 1;
              const more = f.rows.length - FACET_ROWS;
              return (
                <section key={f.key} style={{ border: "1px solid var(--line)", borderRadius: 6, paddingBottom: 6, background: "var(--panel)" }}>
                  <header
                    style={{
                      display: "grid",
                      gridTemplateColumns: COLS,
                      gap: 8,
                      alignItems: "center",
                      padding: "6px 12px",
                      borderBottom: "1px solid var(--line)",
                    }}
                  >
                    <strong style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{f.label}</strong>
                    <span style={{ textAlign: "right" }}>{fmtInt(f.turnsNow)}</span>
                    <span style={{ textAlign: "right", color: "var(--muted)" }}>{fmtAut(f.autonomy)}</span>
                    <Bar parts={f.byClass} colors={colors.cls} total={f.turnsNow} scale={f.turnsNow > 0 ? 1 : 0} keys={CLASS_KEYS} />
                    <Bar parts={f.byOrigin} colors={colors.origin} total={f.turnsNow} scale={f.turnsNow > 0 ? 1 : 0} keys={ORIGIN_KEYS} />
                  </header>
                  {f.rows.slice(0, FACET_ROWS).map((r) => (
                    <Row key={r.i} {...rowProps(r, fmax)} />
                  ))}
                  {more > 0 && (
                    <button
                      type="button"
                      className="ca-btn"
                      data-variant="ghost"
                      disabled={!f.filter}
                      style={{ margin: "4px 12px 0" }}
                      onClick={() => {
                        if (f.filter) setFilters(f.filter);
                        setFacet("none");
                      }}
                    >
                      +{fmtInt(more)} more{f.filter ? " · show all (filters to this)" : ""}
                    </button>
                  )}
                </section>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
