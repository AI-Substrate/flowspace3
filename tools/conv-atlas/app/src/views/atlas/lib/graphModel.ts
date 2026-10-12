/**
 * Similarity graph model: conversations as nodes, kNN edges (centred cosine)
 * as weighted links, seeded for ForceAtlas2 from the galaxy layout, or from the
 * previous snapshot's settled positions so a swap does not scramble the graph.
 */
import type { LoadedSnapshot } from "../../../core/data";
import { buildLayout } from "./galaxyModel";
import { type TimeOrder, timeOrder } from "./gate";

/** graph units per galaxy point-space unit */
export const GRAPH_SCALE = 100;

export interface SimNode {
  guid: string;
  /** conversation index */
  ci: number;
  x: number;
  y: number;
}

export interface SimEdge {
  /** conversation indices, a < b */
  a: number;
  b: number;
  /** centred cosine, floored so ForceAtlas2 always attracts */
  weight: number;
  /** links two different repos */
  cross: boolean;
  /** position in birth order: visible at the playhead iff rank < countAtOrBefore(order.times, cursor) */
  rank: number;
  /** where the weight falls among this snapshot's edges, 0 (weakest) to 1 (strongest); drives link fading */
  strength: number;
}

export interface SimGraph {
  nodes: SimNode[];
  edges: SimEdge[];
  /** edges by birth time (t_first) */
  order: TimeOrder;
}

export const MIN_WEIGHT = 0.01;

/**
 * Nodes are conversations with a layout position or at least one kNN edge.
 * Seeds: previous position (by guid) > galaxy xy > mean of placed neighbours > a
 * deterministic spot near the origin.
 */
export function similarityGraph(s: LoadedSnapshot, previous?: ReadonlyMap<string, readonly [number, number]>): SimGraph {
  const { conversations } = s;
  const knn = s.edges.knn.edges;
  const layout = buildLayout(s);
  const order = timeOrder(knn.length, (e) => knn[e][3]);
  const rank = new Uint32Array(knn.length);
  order.order.forEach((e, k) => {
    rank[e] = k;
  });

  const inGraph = new Uint8Array(conversations.length);
  for (let i = 0; i < conversations.length; i++) if (conversations[i].xy) inGraph[i] = 1;
  for (const [a, b] of knn) inGraph[a] = inGraph[b] = 1;

  const pos = new Map<number, [number, number]>();
  for (let i = 0; i < conversations.length; i++) {
    if (!inGraph[i]) continue;
    const prev = previous?.get(conversations[i].guid);
    const p = layout.pointOf[i];
    if (prev) pos.set(i, [prev[0], prev[1]]);
    else if (p >= 0) pos.set(i, [layout.x[p] * GRAPH_SCALE, layout.y[p] * GRAPH_SCALE]);
  }
  // unplaced nodes: mean of placed neighbours, else a golden-angle spiral near the origin
  const sum = new Map<number, [number, number, number]>();
  for (const [a, b] of knn) {
    for (const [u, v] of [
      [a, b],
      [b, a],
    ]) {
      const pv = pos.get(v);
      if (pos.has(u) || !pv) continue;
      const acc = sum.get(u) ?? [0, 0, 0];
      sum.set(u, [acc[0] + pv[0], acc[1] + pv[1], acc[2] + 1]);
    }
  }
  const nodes: SimNode[] = [];
  let spiral = 0;
  for (let i = 0; i < conversations.length; i++) {
    if (!inGraph[i]) continue;
    let p = pos.get(i);
    if (!p) {
      const acc = sum.get(i);
      if (acc) p = [acc[0] / acc[2], acc[1] / acc[2]];
      else {
        const r = 4 * Math.sqrt(++spiral);
        const a = spiral * 2.39996;
        p = [r * Math.cos(a), r * Math.sin(a)];
      }
    }
    nodes.push({ guid: conversations[i].guid, ci: i, x: p[0], y: p[1] });
  }

  // weight quantiles: centred cosines bunch together, so fading by raw weight would barely separate them
  const byWeight = knn.map((_, e) => e).sort((x, y) => knn[x][2] - knn[y][2]);
  const strength = new Float64Array(knn.length);
  byWeight.forEach((e, k) => {
    strength[e] = knn.length > 1 ? k / (knn.length - 1) : 1;
  });
  const edges: SimEdge[] = knn.map(([a, b, w], e) => {
    const ra = conversations[a].repo;
    const rb = conversations[b].repo;
    return { a, b, weight: Math.max(MIN_WEIGHT, w), cross: ra !== null && rb !== null && ra !== rb, rank: rank[e], strength: strength[e] };
  });
  return { nodes, edges, order };
}
