/**
 * Pure data transforms behind the Chronos views. No React, no DOM: every
 * function here is deterministic and unit-tested in transforms.test.ts.
 */
import * as d3 from "d3";
import type { LoadedSnapshot } from "../../core/data";
import type { Ms, Session, SessionTurn, TurnCounts } from "../../contract/types";
import { DAY_MS, type Filters, dayOf, existsAt, filterPredicate, turnsUpTo } from "../../core/semantics";

// ── timeline ────────────────────────────────────────────────────────────────

export type GroupBy = "repo" | "harness" | "community" | "kind";

export interface StreamGroup {
  key: string;
  label: string;
  /** stable colour key: repo key, harness name, String(community id), kind; null = no stable colour */
  colorKey: string | null;
  /** the filter patch that isolates this group; null = not filterable ("other", no repo, …) */
  filter: Partial<Filters> | null;
  total: number;
}

export interface DailyMatrix {
  days: number;
  groups: StreamGroup[];
  /** row-major [day][group]: values[day * groups.length + g] */
  values: Float64Array;
}

export const OTHER_KEY = "other";

type GroupId = Omit<StreamGroup, "total">;

const fmtUtcDay = d3.utcFormat("%-d %b %Y");

/**
 * "through 12 Oct 2026 (UTC)": the label for counts binned by UTC day up to the playhead.
 * The cursor is clamped to the newest turn, so no label claims a day after the data ends.
 */
export function throughDay(s: LoadedSnapshot, cursorMs: Ms): string {
  const day = dayOf(s, Math.min(cursorMs, s.meta.end_ms));
  return `through ${fmtUtcDay(new Date(s.meta.day0_ms + day * DAY_MS))} (UTC)`;
}

export function groupOf(s: LoadedSnapshot, i: number, by: GroupBy): GroupId {
  const c = s.conversations[i];
  const m = s.meta;
  switch (by) {
    case "repo": {
      if (c.repo === null) return { key: "repo:", label: "(no repo)", colorKey: null, filter: null };
      const r = m.repos[c.repo];
      return { key: `repo:${r.key}`, label: r.label, colorKey: r.key, filter: { repos: [r.key] } };
    }
    case "harness": {
      if (c.harness === null) return { key: "harness:", label: "(no harness)", colorKey: null, filter: null };
      const h = m.harnesses[c.harness];
      return { key: `harness:${h}`, label: h, colorKey: h, filter: { harnesses: [h] } };
    }
    case "community": {
      if (c.community === null) return { key: "community:", label: "(unclustered)", colorKey: null, filter: null };
      const label = m.communities[c.community]?.label ?? `community ${c.community}`;
      return { key: `community:${c.community}`, label, colorKey: String(c.community), filter: { communities: [c.community] } };
    }
    case "kind":
      return {
        key: `kind:${c.kind}`,
        label: c.kind,
        colorKey: c.kind,
        filter: { kinds: [c.kind] },
      };
  }
}

/**
 * Daily turns per group for the conversations that pass `pass`. Groups are
 * ranked by total turns (ties by key); beyond `topN` the tail collapses into
 * one "other" group, kept last.
 */
export function dailyMatrix(
  s: LoadedSnapshot,
  pass: (i: number) => boolean,
  by: GroupBy,
  topN = 12,
): DailyMatrix {
  const days = s.meta.days;
  const acc = new Map<string, { id: GroupId; total: number; perDay: Float64Array }>();
  for (let i = 0; i < s.conversations.length; i++) {
    if (!pass(i)) continue;
    const id = groupOf(s, i, by);
    let e = acc.get(id.key);
    if (!e) {
      e = { id, total: 0, perDay: new Float64Array(days) };
      acc.set(id.key, e);
    }
    for (const [d, n] of s.conversations[i].activity) {
      if (d < 0 || d >= days) continue;
      e.perDay[d] += n;
      e.total += n;
    }
  }
  const ranked = [...acc.values()].sort((a, b) => b.total - a.total || (a.id.key < b.id.key ? -1 : 1));
  const keep = ranked.length > topN ? ranked.slice(0, Math.max(topN - 1, 0)) : ranked;
  const rest = ranked.slice(keep.length);
  const groups: StreamGroup[] = keep.map((e) => ({ ...e.id, total: e.total }));
  const rows = keep.map((e) => e.perDay);
  if (rest.length) {
    const perDay = new Float64Array(days);
    let total = 0;
    for (const e of rest) {
      for (let d = 0; d < days; d++) perDay[d] += e.perDay[d];
      total += e.total;
    }
    groups.push({ key: OTHER_KEY, label: `other (${rest.length})`, colorKey: null, filter: null, total });
    rows.push(perDay);
  }
  const G = groups.length;
  const values = new Float64Array(days * G);
  for (let g = 0; g < G; g++) for (let d = 0; d < days; d++) values[d * G + g] = rows[g][d];
  return { days, groups, values };
}

/** Centred moving average over `window` days per group; edges average what exists. */
export function smoothDaily(m: DailyMatrix, window: number): DailyMatrix {
  if (window <= 1) return m;
  const G = m.groups.length;
  const half = Math.floor(window / 2);
  const out = new Float64Array(m.values.length);
  for (let g = 0; g < G; g++) {
    for (let d = 0; d < m.days; d++) {
      const lo = Math.max(0, d - half);
      const hi = Math.min(m.days - 1, d + half);
      let sum = 0;
      for (let k = lo; k <= hi; k++) sum += m.values[k * G + g];
      out[d * G + g] = sum / (hi - lo + 1);
    }
  }
  return { ...m, values: out };
}

export type StackMode = "stream" | "stacked";

/** d3 layers over day indices; layer g, point d = [y0, y1]. */
export function stackLayers(m: DailyMatrix, mode: StackMode): d3.Series<number, number>[] {
  const G = m.groups.length;
  const stack = d3
    .stack<number, number>()
    .keys(d3.range(G))
    .value((d, g) => m.values[d * G + g]);
  // silhouette, not wiggle: wiggle drifts the baseline with growth and reads as a fall
  if (mode === "stream") stack.offset(d3.stackOffsetSilhouette).order(d3.stackOrderInsideOut);
  else stack.offset(d3.stackOffsetNone).order(d3.stackOrderNone);
  return stack(d3.range(m.days));
}

/** True when every key of the group's filter patch equals the current filter. */
export function filterIsActive(f: Filters, patch: Partial<Filters> | null): boolean {
  if (!patch) return false;
  return (Object.keys(patch) as (keyof Filters)[]).every((k) => {
    const want = patch[k];
    const have = f[k];
    return Array.isArray(want) && Array.isArray(have)
      ? want.length === have.length && want.every((v, i) => v === have[i])
      : want === have;
  });
}

// ── river ───────────────────────────────────────────────────────────────────

export interface ShiftMarker {
  /** index into session.turns */
  idx: number;
  n: number;
  label: string;
  score: number;
}

/**
 * Place each shift on the turn axis. Shifts are keyed by turn number, which is
 * sparse in `turns` (not every turn ships), so a shift lands on its exact turn
 * or the nearest shipped turn by number (the earlier one on a tie). Shifts
 * outside the shipped turn span are dropped rather than pinned to an edge.
 */
export function shiftMarkers(session: Session): ShiftMarker[] {
  const { turns } = session;
  if (!turns.length) return [];
  const first = turns[0].n;
  const last = turns[turns.length - 1].n;
  const out: ShiftMarker[] = [];
  for (const s of session.shifts) {
    if (s.n < first || s.n > last) continue;
    const hi = d3.bisector((t: SessionTurn) => t.n).left(turns, s.n);
    const idx = turns[hi].n === s.n || hi === 0 ? hi : s.n - turns[hi - 1].n <= turns[hi].n - s.n ? hi - 1 : hi;
    out.push({ idx, n: s.n, label: s.label, score: s.score });
  }
  return out.sort((a, b) => a.idx - b.idx);
}

/**
 * Greedy lane assignment for labels on one axis: each label (start x, width)
 * takes the lowest lane whose previous label ends at least `gap` before it.
 * Input must be sorted by x.
 */
export function labelLanes(spans: { x: number; width: number }[], gap = 6): number[] {
  const laneEnd: number[] = [];
  return spans.map(({ x, width }) => {
    let lane = laneEnd.findIndex((end) => end + gap <= x);
    if (lane < 0) lane = laneEnd.length;
    laneEnd[lane] = x + width;
    return lane;
  });
}

/** Index of the last turn at or before the cursor; -1 before the session starts. */
export function turnIndexAt(turns: SessionTurn[], cursorMs: Ms): number {
  return d3.bisector((t: SessionTurn) => t.t_ms).right(turns, cursorMs) - 1;
}

// ── composition ─────────────────────────────────────────────────────────────

export const CLASS_KEYS = ["work", "tool_call", "tool_result", "retrieval"] as const;
export const ORIGIN_KEYS = ["human", "peer", "agent"] as const;

/**
 * Autonomy index: human turns per 100 tool_call turns (lower = more autonomous).
 * The denominator is a turn class, not the agent origin: harnesses disagree on the
 * origin of tool results (omp: agent, claude: human), so `n_human / n_agent` is
 * harness-biased, while a tool call is a tool call everywhere.
 * null when there are no tool calls, where the ratio means nothing.
 */
export function autonomyIndex(c: Pick<TurnCounts, "human" | "tool_call">): number | null {
  return c.tool_call > 0 ? (c.human / c.tool_call) * 100 : null;
}

/**
 * Split `total` across `keys` in proportion to the final counts ("final mix":
 * the view layer has no per-day class breakdown, so the mix is the
 * conversation's whole-life mix scaled to the turns that have happened).
 */
export function scaleMix<K extends keyof TurnCounts>(counts: TurnCounts, keys: readonly K[], total: number): number[] {
  const sum = keys.reduce((n, k) => n + counts[k], 0);
  return keys.map((k) => (sum > 0 ? (counts[k] / sum) * total : 0));
}

export interface CompositionRow {
  i: number;
  /** turns that had happened by the playhead's day */
  turnsNow: number;
  /** CLASS_KEYS order, scaled to turnsNow */
  byClass: number[];
  /** ORIGIN_KEYS order, scaled to turnsNow */
  byOrigin: number[];
  autonomy: number | null;
  humanShare: number;
}

/** One row per conversation that passes the filters and exists at the playhead. */
export function compositionRows(s: LoadedSnapshot, pass: (i: number) => boolean, cursorMs: Ms): CompositionRow[] {
  const day = dayOf(s, cursorMs);
  const rows: CompositionRow[] = [];
  s.conversations.forEach((c, i) => {
    if (!pass(i) || !existsAt(c, cursorMs)) return;
    const turnsNow = turnsUpTo(s, i, day);
    const origin = c.counts.human + c.counts.peer + c.counts.agent;
    rows.push({
      i,
      turnsNow,
      byClass: scaleMix(c.counts, CLASS_KEYS, turnsNow),
      byOrigin: scaleMix(c.counts, ORIGIN_KEYS, turnsNow),
      autonomy: autonomyIndex(c.counts),
      humanShare: origin > 0 ? c.counts.human / origin : 0,
    });
  });
  return rows;
}

export type CompositionSort = "turns" | "autonomy" | "human share" | "started" | "label";
export const COMPOSITION_SORTS: readonly CompositionSort[] = ["turns", "autonomy", "human share", "started", "label"];

/**
 * Sorted copy. turns / human share: descending; autonomy: ascending (most
 * autonomous first) with null last; started: ascending; label: A→Z. Ties keep
 * snapshot order, so the list does not shuffle while the playhead moves.
 */
export function sortRows(rows: CompositionRow[], s: LoadedSnapshot, by: CompositionSort): CompositionRow[] {
  const cs = s.conversations;
  const cmp: Record<CompositionSort, (a: CompositionRow, b: CompositionRow) => number> = {
    turns: (a, b) => b.turnsNow - a.turnsNow,
    autonomy: (a, b) =>
      a.autonomy === null || b.autonomy === null
        ? Number(a.autonomy === null) - Number(b.autonomy === null)
        : a.autonomy - b.autonomy,
    "human share": (a, b) => b.humanShare - a.humanShare,
    started: (a, b) => cs[a.i].started_ms - cs[b.i].started_ms,
    label: (a, b) => cs[a.i].label.localeCompare(cs[b.i].label),
  };
  return [...rows].sort((a, b) => cmp[by](a, b) || a.i - b.i);
}

export interface Facet {
  key: string;
  label: string;
  colorKey: string | null;
  /** the filter patch that isolates this facet; null for "(no repo)" etc. */
  filter: Partial<Filters> | null;
  rows: CompositionRow[];
  turnsNow: number;
  byClass: number[];
  byOrigin: number[];
  /** human per 100 tool_call turns over the facet's summed final counts */
  autonomy: number | null;
}

/**
 * Rows grouped by repo or harness ("all": one facet over every row), facets ordered by
 * turns at the playhead. Row order is kept. Autonomy is Σ raw human / Σ raw tool_call over
 * the facet's final counts: a shared playhead factor would cancel in the ratio, and the
 * turn-scaled mixes would reweight each conversation.
 */
export function facetRows(rows: CompositionRow[], s: LoadedSnapshot, by: "repo" | "harness" | "all"): Facet[] {
  const facets = new Map<string, Facet & { human: number; tool_call: number }>();
  for (const r of rows) {
    const id: GroupId = by === "all" ? { key: "all", label: "All", colorKey: null, filter: null } : groupOf(s, r.i, by);
    let f = facets.get(id.key);
    if (!f) {
      f = { ...id, rows: [], turnsNow: 0, byClass: [0, 0, 0, 0], byOrigin: [0, 0, 0], autonomy: null, human: 0, tool_call: 0 };
      facets.set(id.key, f);
    }
    f.rows.push(r);
    f.turnsNow += r.turnsNow;
    r.byClass.forEach((v, k) => (f.byClass[k] += v));
    r.byOrigin.forEach((v, k) => (f.byOrigin[k] += v));
    f.human += s.conversations[r.i].counts.human;
    f.tool_call += s.conversations[r.i].counts.tool_call;
  }
  return [...facets.values()]
    .map(({ human, tool_call, ...f }) => ({ ...f, autonomy: autonomyIndex({ human, tool_call }) }))
    .sort((a, b) => b.turnsNow - a.turnsNow || (a.key < b.key ? -1 : 1));
}

// ── tags ────────────────────────────────────────────────────────────────────

export interface TagMatrix {
  /** top tags, most conversations first */
  tags: string[];
  /** conversations carrying each tag, aligned with `tags` */
  n: number[];
  /** symmetric N×N row-major; [i][j] = conversations carrying both; diagonal = n */
  cooc: Float64Array;
  /** conversations the counts were taken over */
  conversations: number;
  /** "snapshot": the exporter's tags.cooc; "recomputed": from conversation.tags under filters/playhead */
  source: "snapshot" | "recomputed";
}

function topTags(vocab: string[], counts: ArrayLike<number>, topN: number): number[] {
  return d3
    .range(vocab.length)
    .filter((i) => counts[i] > 0)
    .sort((a, b) => counts[b] - counts[a] || (vocab[a] < vocab[b] ? -1 : 1))
    .slice(0, topN);
}

function squareFrom(top: number[], counts: ArrayLike<number>, pairs: Iterable<[number, number, number]>): Float64Array {
  const N = top.length;
  const pos = new Map(top.map((v, k) => [v, k]));
  const m = new Float64Array(N * N);
  top.forEach((v, k) => (m[k * N + k] = counts[v]));
  for (const [a, b, n] of pairs) {
    const i = pos.get(a);
    const j = pos.get(b);
    if (i === undefined || j === undefined) continue;
    m[i * N + j] = n;
    m[j * N + i] = n;
  }
  return m;
}

/** The exporter's conversation-level co-occurrence for the whole snapshot. */
export function snapshotTagMatrix(s: LoadedSnapshot, topN: number): TagMatrix {
  const vocab = s.tags.tags.map((t) => t.tag);
  const counts = s.tags.tags.map((t) => t.conversations);
  const top = topTags(vocab, counts, topN);
  return {
    tags: top.map((v) => vocab[v]),
    n: top.map((v) => counts[v]),
    cooc: squareFrom(top, counts, s.tags.cooc),
    conversations: s.conversations.length,
    source: "snapshot",
  };
}

/**
 * Co-occurrence recomputed from `conversation.tags` over the conversations that
 * pass the filters and exist at the playhead. Only tags in the snapshot's tag
 * vocabulary count, so labels agree with the snapshot matrix.
 */
export function recomputedTagMatrix(s: LoadedSnapshot, pass: (i: number) => boolean, cursorMs: Ms, topN: number): TagMatrix {
  const vocab = s.tags.tags.map((t) => t.tag);
  const V = vocab.length;
  const index = new Map(vocab.map((t, i) => [t, i]));
  const counts = new Float64Array(V);
  const pairs = new Map<number, number>();
  let conversations = 0;
  s.conversations.forEach((c, ci) => {
    if (!pass(ci) || !existsAt(c, cursorMs)) return;
    conversations++;
    const ids = [...new Set(c.tags.map((t) => index.get(t)).filter((v): v is number => v !== undefined))].sort((a, b) => a - b);
    for (let x = 0; x < ids.length; x++) {
      counts[ids[x]]++;
      for (let y = x + 1; y < ids.length; y++) {
        const key = ids[x] * V + ids[y];
        pairs.set(key, (pairs.get(key) ?? 0) + 1);
      }
    }
  });
  const top = topTags(vocab, counts, topN);
  const triples = [...pairs].map(([key, n]): [number, number, number] => [Math.floor(key / V), key % V, n]);
  return {
    tags: top.map((v) => vocab[v]),
    n: top.map((v) => counts[v]),
    cooc: squareFrom(top, counts, triples),
    conversations,
    source: "recomputed",
  };
}

/** Jaccard association |A∩B| / |A∪B| per cell; 1 on the diagonal. Frequency-independent. */
export function jaccard(m: TagMatrix): Float64Array {
  const N = m.tags.length;
  const out = new Float64Array(N * N);
  for (let i = 0; i < N; i++)
    for (let j = 0; j < N; j++) {
      const both = m.cooc[i * N + j];
      const union = m.n[i] + m.n[j] - both;
      out[i * N + j] = i === j ? 1 : union > 0 ? both / union : 0;
    }
  return out;
}

/**
 * Seriation by average-linkage (UPGMA) clustering on distance 1 − similarity:
 * the leaf order puts tags that travel together next to each other, so blocks
 * show up on the matrix diagonal. Deterministic: ties merge the lowest indices.
 */
export function clusterOrder(sim: Float64Array, N: number): number[] {
  const members: (number[] | null)[] = d3.range(N).map((i) => [i]);
  const d = new Float64Array(N * N);
  for (let i = 0; i < N * N; i++) d[i] = 1 - sim[i];
  for (let step = 1; step < N; step++) {
    let best = Number.POSITIVE_INFINITY;
    let a = -1;
    let b = -1;
    for (let i = 0; i < N; i++) {
      if (!members[i]) continue;
      for (let j = i + 1; j < N; j++) {
        if (members[j] && d[i * N + j] < best) {
          best = d[i * N + j];
          a = i;
          b = j;
        }
      }
    }
    const A = members[a] as number[];
    const B = members[b] as number[];
    for (let k = 0; k < N; k++) {
      if (!members[k] || k === a || k === b) continue;
      const v = (A.length * d[a * N + k] + B.length * d[b * N + k]) / (A.length + B.length);
      d[a * N + k] = v;
      d[k * N + a] = v;
    }
    members[a] = A.concat(B);
    members[b] = null;
  }
  return members.find((m) => m !== null) ?? [];
}

// ---------------------------------------------------------------- overview

/** A count keyed by an index into a meta array; `null` = unknown. */
export interface Tally {
  i: number | null;
  n: number;
}

/** Peer messages per seat; `i: null` is the single "unknown" row (unknown sender or receiver). */
export interface SeatTally {
  i: number | null;
  sent: number;
  received: number;
}

/** Landing numbers for the conversations that pass the filters and exist at the playhead. */
export interface Overview {
  conversations: number;
  subagents: number;
  /** turns that had happened by the end of the playhead's day */
  turns: number;
  /** seats whose first evidence is at or before the playhead (seat filter applied) */
  seats: number;
  /** of those, seats that own a conversation passing the filters */
  seatsWithConversation: number;
  repos: number;
  /** turns per day over the whole snapshot, for passing conversations (not clipped to the playhead) */
  daily: Float64Array;
  /** most conversations first; unclustered conversations are counted in `unclustered` instead */
  communities: Tally[];
  unclustered: number;
  harnesses: Tally[];
  /** conversations using each model; one conversation can use several */
  models: Tally[];
  /**
   * Peer messages through the playhead's day. A message counts when its receiving
   * conversation passes every filter except seats; the seat filter applies to its
   * endpoints (sender or receiver). Unknown endpoints tally on the `i: null` row.
   */
  seatMessages: SeatTally[];
}

const bump = <K>(m: Map<K, number>, k: K, n = 1) => m.set(k, (m.get(k) ?? 0) + n);

/** Map entries as tallies, largest first; ties keep index order with unknown last. */
function ranked(m: Map<number | null, number>): Tally[] {
  return [...m]
    .map(([i, n]) => ({ i, n }))
    .sort((a, b) => b.n - a.n || (a.i ?? Number.MAX_SAFE_INTEGER) - (b.i ?? Number.MAX_SAFE_INTEGER));
}

export function overview(s: LoadedSnapshot, f: Filters, cursorMs: Ms): Overview {
  const pass = filterPredicate(s, f);
  const day = dayOf(s, cursorMs);
  const daily = new Float64Array(s.meta.days);
  const communities = new Map<number | null, number>();
  const harnesses = new Map<number | null, number>();
  const models = new Map<number | null, number>();
  const seats = new Set<number>();
  const repos = new Set<number>();
  let conversations = 0;
  let subagents = 0;
  let turns = 0;
  let unclustered = 0;
  s.conversations.forEach((c, i) => {
    if (!pass(i)) return;
    for (const [d, n] of c.activity) if (d >= 0 && d < daily.length) daily[d] += n;
    if (!existsAt(c, cursorMs)) return;
    conversations++;
    if (c.kind === "subagent") subagents++;
    turns += turnsUpTo(s, i, day);
    if (c.seat !== null) seats.add(c.seat);
    if (c.repo !== null) repos.add(c.repo);
    if (c.community === null) unclustered++;
    else bump(communities, c.community);
    bump(harnesses, c.harness);
    for (const m of c.models) bump(models, m);
  });
  const seatFilter = new Set(f.seats);
  const inSeatFilter = (seat: number | null) => seat !== null && seatFilter.has(s.meta.seats[seat].id);
  const passReceiver = filterPredicate(s, { ...f, seats: [] });
  const sent = new Map<number | null, number>();
  const received = new Map<number | null, number>();
  for (const m of s.edges.messages) {
    if (m.day > day || !passReceiver(m.to_conv)) continue;
    if (seatFilter.size && !inSeatFilter(m.from) && !inSeatFilter(m.to)) continue;
    bump(sent, m.from, m.n);
    bump(received, m.to, m.n);
  }
  const seatMessages = [...new Set([...sent.keys(), ...received.keys()])]
    .map((i) => ({ i, sent: sent.get(i) ?? 0, received: received.get(i) ?? 0 }))
    .sort((a, b) => b.sent + b.received - (a.sent + a.received) || (a.i ?? Number.MAX_SAFE_INTEGER) - (b.i ?? Number.MAX_SAFE_INTEGER));
  return {
    conversations,
    subagents,
    turns,
    seats: s.meta.seats.filter((seat) => seat.first_ms <= cursorMs && (!seatFilter.size || seatFilter.has(seat.id))).length,
    seatsWithConversation: seats.size,
    repos: repos.size,
    daily,
    communities: ranked(communities),
    unclustered,
    harnesses: ranked(harnesses),
    models: ranked(models),
    seatMessages,
  };
}

/** A corpus-level caveat read from meta.stats. `text` is null when the snapshot did not record it. */
export interface Caveat {
  title: string;
  text: string | null;
  /** the recorded numbers call for caution */
  warn: boolean;
}

/** Below this share of work turns summarised or vectorised, coverage is flagged. */
const COVERAGE_WARN = 0.9;

/** Coverage, harness confound, DL-029 and omp role attribution, from the flat meta.stats keys agreed with Cartographer. */
export function caveats(stats: Record<string, number>): Caveat[] {
  const pct = d3.format(".0%");
  const summary = stats.coverage_summary;
  const vector = stats.coverage_any_vector;
  const coverage = [
    summary === undefined ? null : `${pct(summary)} of work turns are summarised`,
    vector === undefined ? null : `${pct(vector)} have a vector`,
  ].filter((p) => p !== null);
  const same = stats.harness_confound_same_harness_edge_share;
  const base = stats.harness_confound_random_baseline;
  const dl029 = stats.dl029_tool_call_turns;
  const ompZeroHuman = stats.omp_sessions_zero_human;
  return [
    {
      title: "Coverage",
      text: coverage.length ? `${coverage.join("; ")}. Unsummarised turns carry no tags or label text.` : null,
      warn: Math.min(summary ?? 1, vector ?? 1) < COVERAGE_WARN,
    },
    {
      title: "Harness confound",
      text:
        same === undefined
          ? null
          : `${pct(same)} of kNN edges join the same harness${base === undefined ? "" : `, against ${pct(base)} by chance`}. Compare within one harness before calling two conversations alike.`,
      warn: same !== undefined && (base === undefined || same > base),
    },
    {
      title: "DL-029 · tool_call text",
      text:
        dl029 === undefined
          ? null
          : dl029 > 0
            ? `${d3.format(",")(dl029)} tool_call turns hold whole file bodies (the elision bug), so tool calls are counted but never embedded.`
            : "No tool_call turns are affected in this snapshot.",
      warn: (dl029 ?? 0) > 0,
    },
    {
      title: "omp human/peer attribution",
      text:
        ompZeroHuman === undefined
          ? null
          : ompZeroHuman > 0
            ? `${d3.format(",")(ompZeroHuman)} omp sessions have no human turns: omp sessions from 2026-09-07 store human/peer turns as agent (fs3 intake bug); human, peer and autonomy figures for omp are unreliable.`
            : "Every omp session has human turns.",
      warn: (ompZeroHuman ?? 0) > 0,
    },
  ];
}
