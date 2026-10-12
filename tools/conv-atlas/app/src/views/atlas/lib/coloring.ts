/**
 * Colour coding shared by the atlas views: maps the store's colorBy to a
 * category per conversation, a theme-aware colour per category, and a legend
 * sorted by count. Community and repo colours come from the snapshot's sticky
 * core slots (`snapshot.slots`), shared with every view and stable across swaps;
 * harness and kind use the palette's fixed per-name colours.
 */
import type { LoadedSnapshot } from "../../../core/data";
import { DAY_MS } from "../../../core/semantics";
import type { ColorBy } from "../../../core/store";
import { ageRamp, categorical, communityColor, dimmed, harnessColor, kindColor, neutral, type RGBA } from "../../../design/palette";
import type { Theme } from "../../../design/theme";

export const AGE_BINS = 24;

export interface LegendEntry {
  /** filter key for this category (community index, repo key, harness, kind); null = not filterable */
  key: string | null;
  label: string;
  color: RGBA;
  /** conversations in this category */
  count: number;
}

/** A colour-coding legend row: also names its category, so views can recount it. */
export interface CodedLegendEntry extends LegendEntry {
  category: number;
}

export interface ColorCoding {
  /** category per conversation, before filters */
  category: Uint16Array;
  /** colour per category; the last two are "none" (no value) and the ghost */
  colors: RGBA[];
  /** category index for filtered-out marks */
  ghost: number;
  /** categories by descending snapshot-wide count; age lists its ends and middle only */
  legend: CodedLegendEntry[];
}

/** Colour categories for the current colorBy and theme. */
export function colorCoding(s: LoadedSnapshot, by: ColorBy, theme: Theme): ColorCoding {
  const { meta, conversations } = s;
  let n: number;
  let category: (ci: number) => number | null;
  let label: (k: number) => string;
  let key: (k: number) => string | null;
  /** `slot` is the sticky palette slot from core colorSlots (community, repo); harness and kind use the palette's fixed colours */
  let color: (k: number, slot: number) => RGBA;
  switch (by) {
    case "community":
      n = meta.communities.length;
      category = (ci) => conversations[ci].community;
      label = (k) => meta.communities[k].label;
      key = String;
      color = (_, slot) => communityColor(slot, theme);
      break;
    case "repo":
      n = meta.repos.length;
      category = (ci) => conversations[ci].repo;
      label = (k) => meta.repos[k].label;
      key = (k) => meta.repos[k].key;
      color = (_, slot) => categorical(slot, theme);
      break;
    case "harness":
      n = meta.harnesses.length;
      category = (ci) => conversations[ci].harness;
      label = (k) => meta.harnesses[k];
      key = (k) => meta.harnesses[k];
      color = (k) => harnessColor(meta.harnesses[k], theme);
      break;
    case "kind": {
      const kinds = ["session", "subagent"] as const;
      n = 2;
      category = (ci) => (conversations[ci].kind === "session" ? 0 : 1);
      label = (k) => kinds[k];
      key = (k) => kinds[k];
      color = (k) => kindColor(kinds[k], theme);
      break;
    }
    case "age": {
      const span = Math.max(1, meta.days) * DAY_MS;
      n = AGE_BINS;
      category = (ci) =>
        Math.min(AGE_BINS - 1, Math.max(0, Math.floor(((conversations[ci].started_ms - meta.day0_ms) / span) * AGE_BINS)));
      label = (k) => new Date(meta.day0_ms + (k / AGE_BINS) * span).toISOString().slice(0, 10);
      key = () => null;
      color = (k) => ageRamp(k / (AGE_BINS - 1), theme);
      break;
    }
  }
  const none = n;
  const ghost = n + 1;
  const counts = new Array<number>(n + 1).fill(0);
  const cat = new Uint16Array(conversations.length);
  conversations.forEach((_, ci) => {
    cat[ci] = category(ci) ?? none;
    counts[cat[ci]]++;
  });
  // sticky slots registered snapshot-wide by core; slots 0-7 (CVD-safe) went to the biggest on first sight
  const slots = by === "community" ? s.slots.community : by === "repo" ? s.slots.repo : null;
  const colors: RGBA[] = Array.from({ length: n }, (_, k) => color(k, slots?.get(key(k) as string) ?? k));
  colors.push(neutral(0.45, theme), dimmed(theme));

  const order =
    by === "age" ? [0, AGE_BINS >> 1, AGE_BINS - 1] : Array.from({ length: n }, (_, k) => k).sort((a, b) => counts[b] - counts[a] || a - b);
  const legend: CodedLegendEntry[] = order.map((k) => ({ key: key(k), label: label(k), color: colors[k], count: counts[k], category: k }));
  if (counts[none]) legend.push({ key: null, label: "none", color: colors[none], count: counts[none], category: none });
  return { category: cat, colors, ghost, legend };
}

/**
 * The legend recounted from per-category counts (e.g. conversations visible at
 * the playhead after filters), keeping the snapshot-wide row order so rows do
 * not reshuffle during playback.
 */
export function recountLegend(color: ColorCoding, counts: ArrayLike<number>): LegendEntry[] {
  return color.legend.map((e) => ({ ...e, count: counts[e.category] }));
}

