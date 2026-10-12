/**
 * Incremental playback gating. During playback the cursor moves a little every
 * frame, so instead of rescanning everything we keep items sorted by their
 * start time and only touch the ones the cursor crossed since the last frame.
 */
import type { Ms } from "../../../contract/types";
import type { LoadedSnapshot } from "../../../core/data";
import { filterPredicate, type Filters } from "../../../core/semantics";

/** Filter result per conversation, 1 = passes. */
export function passMask(s: LoadedSnapshot, f: Filters): Uint8Array {
  const pass = filterPredicate(s, f);
  const out = new Uint8Array(s.conversations.length);
  for (let i = 0; i < out.length; i++) out[i] = pass(i) ? 1 : 0;
  return out;
}

/** Number of entries in ascending `sorted` that are <= t. */
export function countAtOrBefore(sorted: ArrayLike<number>, t: number): number {
  let lo = 0;
  let hi = sorted.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (sorted[mid] <= t) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

/** Item indices ordered by time plus the matching sorted times. */
export interface TimeOrder {
  order: Uint32Array;
  times: Float64Array;
}

export function timeOrder(n: number, timeOf: (i: number) => Ms): TimeOrder {
  const order = new Uint32Array(n);
  for (let i = 0; i < n; i++) order[i] = i;
  order.sort((a, b) => timeOf(a) - timeOf(b));
  const times = new Float64Array(n);
  for (let k = 0; k < n; k++) times[k] = timeOf(order[k]);
  return { order, times };
}

/**
 * Existence (started at or before the cursor) and visibility (exists AND
 * passes filters), kept up to date in O(items crossed) per cursor move.
 * `mask` must agree with core/semantics `visibility()`; the tests hold it to that.
 */
export class PlayheadMask {
  readonly exists: Uint8Array;
  readonly mask: Uint8Array;
  private readonly time: TimeOrder;
  /** filter result per item; replace via setPass */
  pass: Uint8Array;
  private count = 0;

  constructor(startedMs: (i: number) => Ms, n: number, pass: Uint8Array) {
    this.time = timeOrder(n, startedMs);
    this.exists = new Uint8Array(n);
    this.mask = new Uint8Array(n);
    this.pass = pass;
  }

  /** New filter result: rebuilds the mask at the current cursor. */
  setPass(pass: Uint8Array): void {
    this.pass = pass;
    for (let i = 0; i < this.mask.length; i++) this.mask[i] = this.exists[i] & pass[i];
  }

  /** Moves the cursor; returns true when anything started or un-started. */
  seek(cursorMs: Ms): boolean {
    const next = countAtOrBefore(this.time.times, cursorMs);
    const { order } = this.time;
    for (let k = this.count; k < next; k++) {
      const i = order[k];
      this.exists[i] = 1;
      this.mask[i] = this.pass[i];
    }
    for (let k = next; k < this.count; k++) {
      const i = order[k];
      this.exists[i] = 0;
      this.mask[i] = 0;
    }
    const changed = next !== this.count;
    this.count = next;
    return changed;
  }
}
