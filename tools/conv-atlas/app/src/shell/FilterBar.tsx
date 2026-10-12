/**
 * Filter bar: facet menus (repo, harness, seat, community, kind), active
 * filter chips and text search. Writes `setFilters` with stable keys. Counts
 * are faceted (each facet counts as if its own selection were empty) and
 * respect the playhead, so they move during playback (throttled).
 */
import { useEffect, useMemo, useRef, useState } from "react";
import type { LoadedSnapshot } from "../core/data";
import { type Filters, existsAt, filterPredicate, NO_FILTERS, isFiltered } from "../core/semantics";
import { atlas, useAtlas } from "../core/store";
import { categorical, communityColor, harnessColor, kindColor, type RGBA } from "../design/palette";
import { Chip, Swatch } from "../design/primitives";
import { fmt } from "./format";
import { useThrottledAtlas, useTheme } from "./hooks";
import { Chevron, Search } from "./icons";
import { Popover } from "./Popover";

type FacetKey = "repos" | "harnesses" | "seats" | "communities" | "kinds";

interface FacetValue {
  /** the stable filter value (string key, or community index) */
  value: string | number;
  label: string;
  color?: RGBA;
}

interface Facet {
  key: FacetKey;
  title: string;
  values: FacetValue[];
  /** facet value of conversation i, or null */
  of: (i: number) => string | number | null;
}

function facetsOf(snap: LoadedSnapshot): Facet[] {
  const { meta, conversations: cs, slots } = snap;
  return [
    {
      key: "repos",
      title: "Repo",
      values: meta.repos.map((r) => ({ value: r.key, label: r.label, color: categorical(slots.repo.get(r.key) ?? 0) })),
      of: (i) => (cs[i].repo === null ? null : meta.repos[cs[i].repo as number].key),
    },
    {
      key: "harnesses",
      title: "Harness",
      values: meta.harnesses.map((h) => ({ value: h, label: h, color: harnessColor(h) })),
      of: (i) => (cs[i].harness === null ? null : meta.harnesses[cs[i].harness as number]),
    },
    {
      key: "seats",
      title: "Seat",
      values: meta.seats.map((s) => ({ value: s.id, label: s.id })),
      of: (i) => (cs[i].seat === null ? null : meta.seats[cs[i].seat as number].id),
    },
    {
      key: "communities",
      title: "Community",
      values: meta.communities.map((c, i) => ({ value: i, label: c.label, color: communityColor(slots.community.get(String(i)) ?? i) })),
      of: (i) => cs[i].community,
    },
    {
      key: "kinds",
      title: "Kind",
      values: [
        { value: "session", label: "Session", color: kindColor("session") },
        { value: "subagent", label: "Subagent", color: kindColor("subagent") },
      ],
      of: (i) => cs[i].kind,
    },
  ];
}

/** For each facet, value → count of conversations passing every OTHER constraint at the playhead. */
function facetCounts(snap: LoadedSnapshot, facets: Facet[], f: Filters, cursorMs: number) {
  const n = snap.conversations.length;
  const passes = facets.map((fc) => filterPredicate(snap, { ...NO_FILTERS, [fc.key]: f[fc.key] }));
  const query = filterPredicate(snap, { ...NO_FILTERS, query: f.query });
  const counts = facets.map(() => new Map<string | number, number>());
  let shown = 0;
  let existing = 0;
  for (let i = 0; i < n; i++) {
    if (!existsAt(snap.conversations[i], cursorMs)) continue;
    existing++;
    if (!query(i)) continue;
    const ok = passes.map((p) => p(i));
    const failing = ok.reduce((m, v) => m + (v ? 0 : 1), 0);
    if (failing === 0) shown++;
    for (let k = 0; k < facets.length; k++) {
      if (failing === 0 || (failing === 1 && !ok[k])) {
        const v = facets[k].of(i);
        if (v !== null) counts[k].set(v, (counts[k].get(v) ?? 0) + 1);
      }
    }
  }
  return { counts, shown, existing };
}

function FacetMenu({ facet, counts, selected, onToggle }: { facet: Facet; counts: Map<string | number, number>; selected: (string | number)[]; onToggle: (v: string | number) => void }) {
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const anchor = useRef<HTMLButtonElement>(null);
  const values = facet.values
    .filter((v) => !q || v.label.toLowerCase().includes(q.toLowerCase()))
    .sort((a, b) => (counts.get(b.value) ?? 0) - (counts.get(a.value) ?? 0));
  return (
    <>
      <button ref={anchor} type="button" className="sh-facet" aria-expanded={open} aria-haspopup="dialog" onClick={() => setOpen(!open)} data-active={selected.length > 0 || undefined}>
        {facet.title}
        {selected.length ? <span className="sh-count">{selected.length}</span> : null}
        <Chevron />
      </button>
      <Popover anchor={anchor} open={open} onClose={() => setOpen(false)} label={`Filter by ${facet.title.toLowerCase()}`}>
        {facet.values.length > 8 ? (
          <input className="sh-popover__search" placeholder={`Find a ${facet.title.toLowerCase()}`} value={q} onChange={(e) => setQ(e.target.value)} aria-label={`Find a ${facet.title.toLowerCase()}`} />
        ) : null}
        <ul className="sh-optlist" role="group" aria-label={facet.title}>
          {values.map((v) => {
            const on = selected.includes(v.value);
            const c = counts.get(v.value) ?? 0;
            return (
              <li key={String(v.value)}>
                <label className="sh-opt" data-empty={c === 0 || undefined}>
                  <input type="checkbox" checked={on} onChange={() => onToggle(v.value)} />
                  {v.color ? <Swatch color={v.color} /> : null}
                  <span className="sh-opt__label">{v.label}</span>
                  <span className="sh-opt__count">{fmt.count(c)}</span>
                </label>
              </li>
            );
          })}
          {values.length === 0 ? <li className="sh-opt sh-opt--none">Nothing matches “{q}”</li> : null}
        </ul>
      </Popover>
    </>
  );
}

export function FilterBar() {
  const snapshot = useAtlas((s) => s.snapshot);
  const filters = useAtlas((s) => s.filters);
  const setFilters = useAtlas((s) => s.setFilters);
  const cursorDay = useThrottledAtlas(cursorDaySelector, 250);
  const theme = useTheme();
  // facet swatches bake palette colours, so the theme is a dependency
  const facets = useMemo(() => (snapshot ? facetsOf(snapshot) : []), [snapshot, theme]);
  const { counts, shown, existing } = useMemo(
    () => (snapshot ? facetCounts(snapshot, facets, filters, atlas.getState().time.cursorMs) : { counts: [], shown: 0, existing: 0 }),
    // cursorDay re-runs the count when the playhead crosses a day
    [snapshot, facets, filters, cursorDay],
  );

  const [query, setQuery] = useState(filters.query);
  useEffect(() => setQuery(filters.query), [filters.query]);
  useEffect(() => {
    const id = setTimeout(() => {
      if (query !== atlas.getState().filters.query) setFilters({ query });
    }, 150);
    return () => clearTimeout(id);
  }, [query, setFilters]);

  if (!snapshot) return <div className="sh-filters" />;

  const toggle = (key: FacetKey, v: string | number) => {
    const cur = filters[key] as (string | number)[];
    setFilters({ [key]: cur.includes(v) ? cur.filter((x) => x !== v) : [...cur, v] } as Partial<Filters>);
  };
  const active = facets.flatMap((fc) =>
    (filters[fc.key] as (string | number)[]).map((v) => ({ fc, v, meta: fc.values.find((x) => x.value === v) })),
  );

  return (
    <div className="sh-filters" role="search" aria-label="Filter conversations">
      <label className="sh-search">
        <Search />
        <span className="visually-hidden">Search labels, summaries and tags</span>
        <input
          id="sh-search"
          type="search"
          placeholder="Filter by text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              setQuery("");
              e.currentTarget.blur();
            }
          }}
        />
        <kbd className="ca-kbd sh-search__key" aria-hidden="true">
          /
        </kbd>
      </label>
      <div className="sh-facets">
        {facets.map((fc, k) => (
          <FacetMenu key={fc.key} facet={fc} counts={counts[k] ?? new Map()} selected={filters[fc.key] as (string | number)[]} onToggle={(v) => toggle(fc.key, v)} />
        ))}
      </div>
      <div className="sh-active">
        {active.map(({ fc, v, meta }) => (
          <Chip key={`${fc.key}:${v}`} selected color={meta?.color} title={`${fc.title} ${meta?.label ?? v}`} onRemove={() => toggle(fc.key, v)}>
            {meta?.label ?? String(v)}
          </Chip>
        ))}
        {isFiltered(filters) ? (
          <button type="button" className="ca-btn" data-variant="ghost" onClick={() => setFilters(NO_FILTERS)}>
            Clear filters
          </button>
        ) : null}
      </div>
      <p className="sh-tally">
        {isFiltered(filters) ? (
          <>
            <strong>{fmt.count(shown)}</strong> of {fmt.count(existing)} conversations
          </>
        ) : (
          <>
            <strong>{fmt.count(existing)}</strong> conversations
          </>
        )}
      </p>
    </div>
  );
}

const cursorDaySelector = (s: { time: { cursorMs: number } }) => Math.floor(s.time.cursorMs / 86_400_000);
