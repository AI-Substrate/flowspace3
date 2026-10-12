/**
 * Shared semantics every view must agree on: what is visible at the playhead,
 * what passes the filters, and how much of a conversation has happened by now.
 * Pure functions; the store and the views call them, nothing re-implements them.
 */
import type { Conversation, Meta, Ms } from "../contract/types";
import type { LoadedSnapshot } from "./data";

export const DAY_MS = 86_400_000;

/**
 * The snapshot's whole playback range: UTC midnight of day 0 to the newest turn.
 * It ends at the data, not at the end of that UTC day, so no view claims a later date.
 * The store's default range and every "is the range narrowed?" check use this.
 */
export function fullRange(meta: Meta): [Ms, Ms] {
  return [meta.day0_ms, meta.end_ms];
}

/**
 * Filters hold STABLE keys (repo key, harness name, seat id) so they survive a
 * snapshot swap. Communities are snapshot-local and are cleared on swap.
 * An empty list means "no constraint".
 */
export interface Filters {
  repos: string[];
  harnesses: string[];
  seats: string[];
  communities: number[];
  kinds: Conversation["kind"][];
  /** case-insensitive substring over label, blurb and tags */
  query: string;
}

export const NO_FILTERS: Filters = { repos: [], harnesses: [], seats: [], communities: [], kinds: [], query: "" };

export function isFiltered(f: Filters): boolean {
  return (
    f.repos.length + f.harnesses.length + f.seats.length + f.communities.length + f.kinds.length > 0 ||
    f.query.trim() !== ""
  );
}

export function dayOf(snapshot: LoadedSnapshot, t: Ms): number {
  return Math.floor((t - snapshot.meta.day0_ms) / DAY_MS);
}

/** Turns that had happened by the END of day `day` (inclusive). */
export function turnsUpTo(snapshot: LoadedSnapshot, i: number, day: number): number {
  const act = snapshot.conversations[i].activity;
  const prefix = snapshot.activityPrefix[i];
  let lo = 0;
  let hi = act.length - 1;
  let found = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (act[mid][0] <= day) {
      found = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return found < 0 ? 0 : prefix[found];
}

/** A predicate per conversation index, built once per (snapshot, filters). */
export function filterPredicate(snapshot: LoadedSnapshot, f: Filters): (i: number) => boolean {
  const { meta, conversations } = snapshot;
  const repoSet = new Set(f.repos);
  const harnessSet = new Set(f.harnesses);
  const seatSet = new Set(f.seats);
  const commSet = new Set(f.communities);
  const kindSet = new Set(f.kinds);
  const q = f.query.trim().toLowerCase();
  return (i) => {
    const c = conversations[i];
    if (repoSet.size && (c.repo === null || !repoSet.has(meta.repos[c.repo].key))) return false;
    if (harnessSet.size && (c.harness === null || !harnessSet.has(meta.harnesses[c.harness]))) return false;
    if (seatSet.size && (c.seat === null || !seatSet.has(meta.seats[c.seat].id))) return false;
    if (commSet.size && (c.community === null || !commSet.has(c.community))) return false;
    if (kindSet.size && !kindSet.has(c.kind)) return false;
    if (q) {
      const hay = `${c.label}\n${c.blurb ?? ""}\n${c.tags.join(" ")}`.toLowerCase();
      if (!hay.includes(q)) return false;
    }
    return true;
  };
}

/** Exists at the playhead: started at or before the cursor. */
export function existsAt(c: Conversation, cursorMs: Ms): boolean {
  return c.started_ms <= cursorMs;
}

/** Live at the playhead: started and not yet finished. */
export function liveAt(c: Conversation, cursorMs: Ms): boolean {
  return c.started_ms <= cursorMs && cursorMs <= c.last_ms;
}

/**
 * 1 = passes filters AND exists at the playhead. One byte per conversation,
 * so WebGL views can upload it as-is.
 */
export function visibility(snapshot: LoadedSnapshot, f: Filters, cursorMs: Ms, out?: Uint8Array): Uint8Array {
  const n = snapshot.conversations.length;
  const mask = out && out.length === n ? out : new Uint8Array(n);
  const pass = filterPredicate(snapshot, f);
  for (let i = 0; i < n; i++) {
    mask[i] = existsAt(snapshot.conversations[i], cursorMs) && pass(i) ? 1 : 0;
  }
  return mask;
}
