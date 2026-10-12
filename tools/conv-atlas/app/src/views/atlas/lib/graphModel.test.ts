import { describe, expect, it } from "vitest";
import { DAY_MS } from "../../../core/semantics";
import { countAtOrBefore } from "./gate";
import { GRAPH_SCALE, MIN_WEIGHT, similarityGraph } from "./graphModel";
import { conv, DAY0, snapshot } from "./testSnapshot";

describe("similarityGraph", () => {
  const s = snapshot(
    [
      conv({ guid: "a", xy: [1, 0], repo: 0 }),
      conv({ guid: "b", xy: [-1, 0], repo: 1 }),
      conv({ guid: "c", xy: null, repo: 0 }),
      conv({ guid: "lonely", xy: null }),
    ],
    { repos: [{ key: "r0", label: "r0" }, { key: "r1", label: "r1" }] },
  );
  s.edges.knn.edges = [
    [0, 2, 0.4, DAY0 + 5 * DAY_MS],
    [0, 1, -0.2, DAY0 + 1 * DAY_MS],
  ];

  it("keeps placed or linked conversations and drops isolated unplaced ones", () => {
    expect(similarityGraph(s).nodes.map((n) => n.guid)).toEqual(["a", "b", "c"]);
  });

  it("seeds unplaced nodes at the mean of their placed neighbours", () => {
    const g = similarityGraph(s);
    const a = g.nodes.find((n) => n.guid === "a");
    const c = g.nodes.find((n) => n.guid === "c");
    expect([c?.x, c?.y]).toEqual([a?.x, a?.y]);
    expect(a?.x).toBeCloseTo(1.9 / 2 * GRAPH_SCALE);
  });

  it("prefers the previous snapshot's settled position for the same guid", () => {
    const g = similarityGraph(s, new Map([["b", [7, 8]]]));
    const b = g.nodes.find((n) => n.guid === "b");
    expect([b?.x, b?.y]).toEqual([7, 8]);
  });

  it("ranks edges by birth time, floors weights and flags cross-repo links", () => {
    const g = similarityGraph(s);
    expect(g.edges.map((e) => [e.rank, e.weight, e.cross])).toEqual([
      [1, 0.4, false],
      [0, MIN_WEIGHT, true],
    ]);
    // at day 2 only the earlier edge (rank 0) is born
    const count = countAtOrBefore(g.order.times, DAY0 + 2 * DAY_MS);
    expect(g.edges.filter((e) => e.rank < count).map((e) => [e.a, e.b])).toEqual([[0, 1]]);
  });
});
