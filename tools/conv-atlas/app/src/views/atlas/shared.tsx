/**
 * Chrome shared by the atlas views: the colour-by switch, the colour legend
 * whose rows toggle filters, the conversation tooltip body and the playhead date.
 */
import type { LoadedSnapshot } from "../../core/data";
import { dayOf, type Filters, turnsUpTo } from "../../core/semantics";
import { type ColorBy, useAtlas } from "../../core/store";
import { Legend, SegmentedControl } from "../../design/primitives";
import type { LegendEntry } from "./lib/coloring";

const COLOR_OPTIONS: { value: ColorBy; label: string }[] = [
  { value: "community", label: "Community" },
  { value: "repo", label: "Repo" },
  { value: "harness", label: "Harness" },
  { value: "kind", label: "Kind" },
  { value: "age", label: "Age" },
];

/** Which filter list a legend row toggles for each colouring; age is not filterable. */
const FILTER_FIELD: Record<ColorBy, keyof Pick<Filters, "communities" | "repos" | "harnesses" | "kinds"> | null> = {
  community: "communities",
  repo: "repos",
  harness: "harnesses",
  kind: "kinds",
  age: null,
};

export function ColorByControl() {
  const colorBy = useAtlas((s) => s.colorBy);
  const setColorBy = useAtlas((s) => s.setColorBy);
  return <SegmentedControl label="Colour by" size="sm" value={colorBy} options={COLOR_OPTIONS} onChange={setColorBy} />;
}

/**
 * The playhead's UTC date, never later than the newest turn (`meta.end_ms`).
 * The string only changes once per day, so playback re-renders callers daily.
 */
export function usePlayheadDate(): string {
  return useAtlas((s) =>
    s.snapshot ? new Date(Math.min(s.time.cursorMs, s.snapshot.meta.end_ms)).toISOString().slice(0, 10) : "",
  );
}

/** Legend in the stage's bottom-left corner; rows toggle the matching filter (include-list semantics). */
export function ColorLegend({ entries }: { entries: LegendEntry[] }) {
  const colorBy = useAtlas((s) => s.colorBy);
  const filters = useAtlas((s) => s.filters);
  const setFilters = useAtlas((s) => s.setFilters);
  if (!entries.length) return null;
  const field = FILTER_FIELD[colorBy];
  const list = field ? (filters[field] as (string | number)[]) : [];
  const toggle = (key: string) => {
    if (!field) return;
    const value = field === "communities" ? Number(key) : key;
    const next = list.includes(value) ? list.filter((v) => v !== value) : [...list, value];
    setFilters({ [field]: next } as Partial<Filters>);
  };
  return (
    <div style={{ position: "absolute", left: "var(--sp-6)", bottom: "var(--sp-6)", zIndex: "var(--z-stage-overlay)" }}>
      <Legend
        title={COLOR_OPTIONS.find((o) => o.value === colorBy)?.label ?? colorBy}
        items={entries.map((e, i) => ({
          key: e.key ?? `none-${i}`,
          label: e.label,
          color: e.color,
          count: e.count,
          active: !field || e.key === null || list.length === 0 || list.map(String).includes(e.key),
          // "none" is not a filter value
          fixed: e.key === null,
        }))}
        onToggle={field ? toggle : undefined}
        max={10}
      />
    </div>
  );
}

/** Tooltip body for one conversation; day-granular so playback re-renders it once per simulated day. */
export function ConversationTip({ guid, snapshot }: { guid: string; snapshot: LoadedSnapshot }) {
  const day = useAtlas((s) => (s.snapshot ? dayOf(s.snapshot, s.time.cursorMs) : 0));
  const ci = snapshot.guidIndex.get(guid);
  if (ci === undefined) return null;
  const c = snapshot.conversations[ci];
  const { meta } = snapshot;
  const facts = [
    c.repo !== null ? meta.repos[c.repo].label : null,
    c.harness !== null ? meta.harnesses[c.harness] : null,
    c.seat !== null ? meta.seats[c.seat].id : null,
    c.kind,
  ].filter(Boolean);
  const community = c.community !== null ? meta.communities[c.community].label : "unclustered";
  return (
    <>
      <div style={{ fontWeight: "var(--fw-medium)" }}>{c.label}</div>
      <div style={{ color: "var(--muted)" }}>{facts.join(" · ")}</div>
      <div>
        {community} · {turnsUpTo(snapshot, ci, day).toLocaleString()} of {c.turns.toLocaleString()} turns
      </div>
      {c.tags.length ? <div style={{ color: "var(--muted)" }}>{c.tags.slice(0, 5).join(", ")}</div> : null}
    </>
  );
}

/** Interaction hint in the stage's bottom-right corner. */
export function StageHint({ children }: { children: string }) {
  return (
    <p
      style={{
        position: "absolute",
        right: "var(--sp-6)",
        bottom: "var(--sp-4)",
        margin: 0,
        color: "var(--muted)",
        fontSize: "var(--fs-xs)",
        pointerEvents: "none",
      }}
    >
      {children}
    </p>
  );
}
