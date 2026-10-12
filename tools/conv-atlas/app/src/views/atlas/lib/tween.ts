/**
 * Snapshot swap: match the outgoing and incoming point sets by guid so the
 * galaxy can tween instead of popping. The union keeps every incoming point at
 * its own index (0..next.n-1) and appends outgoing-only points after them, so
 * after the tween the tail can simply be dropped.
 */
import type { GalaxyLayout } from "./galaxyModel";

export const MATCHED = 0;
export const ADDED = 1;
export const REMOVED = 2;

export interface TweenUnion {
  /** incoming point count; union indices >= this are REMOVED points */
  nextCount: number;
  status: Uint8Array;
  fromX: Float32Array;
  fromY: Float32Array;
  toX: Float32Array;
  toY: Float32Array;
  /** union index → point index in the outgoing layout, -1 for ADDED */
  prevPoint: Int32Array;
}

export function matchTween(
  prev: GalaxyLayout,
  prevGuids: readonly string[],
  next: GalaxyLayout,
  nextGuids: readonly string[],
): TweenUnion {
  const prevByGuid = new Map<string, number>();
  for (let p = 0; p < prev.convOf.length; p++) prevByGuid.set(prevGuids[prev.convOf[p]], p);

  const nextCount = next.convOf.length;
  const seen = new Uint8Array(prev.convOf.length);
  const removed: number[] = [];
  for (let p = 0; p < nextCount; p++) {
    const q = prevByGuid.get(nextGuids[next.convOf[p]]);
    if (q !== undefined) seen[q] = 1;
  }
  for (let q = 0; q < seen.length; q++) if (!seen[q]) removed.push(q);

  const n = nextCount + removed.length;
  const u: TweenUnion = {
    nextCount,
    status: new Uint8Array(n),
    fromX: new Float32Array(n),
    fromY: new Float32Array(n),
    toX: new Float32Array(n),
    toY: new Float32Array(n),
    prevPoint: new Int32Array(n),
  };
  for (let p = 0; p < nextCount; p++) {
    const q = prevByGuid.get(nextGuids[next.convOf[p]]);
    u.toX[p] = next.x[p];
    u.toY[p] = next.y[p];
    if (q === undefined) {
      u.status[p] = ADDED;
      u.prevPoint[p] = -1;
      u.fromX[p] = next.x[p];
      u.fromY[p] = next.y[p];
    } else {
      u.status[p] = MATCHED;
      u.prevPoint[p] = q;
      u.fromX[p] = prev.x[q];
      u.fromY[p] = prev.y[q];
    }
  }
  removed.forEach((q, r) => {
    const p = nextCount + r;
    u.status[p] = REMOVED;
    u.prevPoint[p] = q;
    u.fromX[p] = u.toX[p] = prev.x[q];
    u.fromY[p] = u.toY[p] = prev.y[q];
  });
  return u;
}

/**
 * Presence (w channel) at both ends of the swap: matched points keep their
 * outgoing presence then tween to the incoming one, added points bloom from 0,
 * removed points fade to 0.
 */
export function tweenPresence(u: TweenUnion, prevW: Float32Array, nextW: Float32Array) {
  const n = u.status.length;
  const from = new Float32Array(n);
  const to = new Float32Array(n);
  for (let p = 0; p < n; p++) {
    const q = u.prevPoint[p];
    from[p] = q >= 0 ? prevW[q] : 0;
    to[p] = p < u.nextCount ? nextW[p] : 0;
  }
  return { from, to };
}
