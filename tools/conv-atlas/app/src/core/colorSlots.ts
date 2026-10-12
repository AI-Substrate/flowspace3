/**
 * Sticky categorical colour slots. A category keeps its palette slot for the life
 * of the page, across snapshot swaps, so a cluster or repo never changes colour
 * under the user's eyes. First sight assigns slots largest-first, so the biggest
 * categories get the colour-blind-safe slots 0-7. Categories seen later take the
 * lowest free slot.
 *
 * Keys must be stable across snapshots: repo key, harness name, kind, seat id, and
 * community_id (the export keeps community ids stable by member overlap).
 */

import type { Conversation, Meta } from "../contract/types";

/** Slots for every per-key categorical dimension of one snapshot, keyed by stable key. */
export interface SnapshotSlots {
  /** repo key -> slot */
  repo: Map<string, number>;
  /** String(community id) -> slot */
  community: Map<string, number>;
  /** model key -> slot */
  model: Map<string, number>;
}

/**
 * Registers every key of a snapshot at once with snapshot-wide counts, so the first
 * sighting ranks the same whatever filter or playhead the first view had. Core calls
 * this once per loaded snapshot (LoadedSnapshot.slots); views read the result.
 */
export function snapshotSlots(meta: Meta, conversations: Conversation[]): SnapshotSlots {
  const repo = new Array<number>(meta.repos.length).fill(0);
  const model = new Array<number>(meta.models.length).fill(0);
  for (const c of conversations) {
    if (c.repo !== null) repo[c.repo]++;
    for (const k of c.models) model[k]++;
  }
  return {
    repo: colorSlots("repo", meta.repos.map((r, i) => ({ key: r.key, count: repo[i] }))),
    community: colorSlots("community", meta.communities.map((c, i) => ({ key: String(i), count: c.size }))),
    model: colorSlots("model", meta.models.map((m, i) => ({ key: m.key, count: model[i] }))),
  };
}

const assigned = new Map<string, Map<string, number>>();

export interface SlotEntry {
  key: string;
  count: number;
}

export function colorSlots(dimension: string, entries: SlotEntry[]): Map<string, number> {
  let slots = assigned.get(dimension);
  if (!slots) {
    slots = new Map();
    assigned.set(dimension, slots);
  }
  const used = new Set(slots.values());
  const fresh = entries
    .filter((e) => !slots.has(e.key))
    .sort((a, b) => b.count - a.count || (a.key < b.key ? -1 : a.key > b.key ? 1 : 0));
  let next = 0;
  for (const e of fresh) {
    while (used.has(next)) next++;
    slots.set(e.key, next);
    used.add(next);
  }
  return new Map(entries.map((e) => [e.key, slots.get(e.key) as number]));
}

/** Test seam: forget every assignment. */
export function resetColorSlots(): void {
  assigned.clear();
}
