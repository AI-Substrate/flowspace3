import { beforeEach, describe, expect, it } from "vitest";
import { resetColorSlots } from "../../core/colorSlots";
import { DAY_MS, NO_FILTERS, filterPredicate } from "../../core/semantics";
import { DAY0, conv, snapshot } from "./testkit";
import {
  OTHER_KEY,
  autonomyIndex,
  caveats,
  clusterOrder,
  compositionRows,
  dailyMatrix,
  facetRows,
  filterIsActive,
  jaccard,
  labelLanes,
  overview,
  recomputedTagMatrix,
  scaleMix,
  shiftMarkers,
  snapshotTagMatrix,
  smoothDaily,
  sortRows,
  stackLayers,
  throughDay,
  turnIndexAt,
} from "./transforms";
import type { Session, SessionTurn } from "../../contract/types";
import type { CompositionSort, TagMatrix } from "./transforms";

const repos = ["a", "b", "c", "d"].map((k) => ({ key: `git:${k}`, label: k }));
const all = () => true;

describe("dailyMatrix", () => {
  const s = snapshot(
    [
      conv({ repo: 0, activity: [[0, 5], [2, 1]] }),
      conv({ repo: 1, activity: [[1, 20]] }),
      conv({ repo: 0, activity: [[2, 4], [99, 1000]] }), // day 99 is outside meta.days
      conv({ repo: null, activity: [[3, 2]] }),
    ],
    { repos, days: 4 },
  );

  it("sums turns per day per group, ranks groups by total, and drops days outside the snapshot", () => {
    const m = dailyMatrix(s, all, "repo");
    expect(m.groups.map((g) => [g.label, g.total])).toEqual([
      ["b", 20],
      ["a", 10],
      ["(no repo)", 2],
    ]);
    const row = (g: number) => [0, 1, 2, 3].map((d) => m.values[d * m.groups.length + g]);
    expect(row(1)).toEqual([5, 0, 5, 0]);
  });

  it("only counts conversations that pass the filter predicate", () => {
    const m = dailyMatrix(s, filterPredicate(s, { ...NO_FILTERS, repos: ["git:a"] }), "repo");
    expect(m.groups.map((g) => g.key)).toEqual(["repo:git:a"]);
  });

  it("collapses the tail beyond topN into a trailing, non-filterable 'other' that keeps the mass", () => {
    const m = dailyMatrix(s, all, "repo", 2);
    expect(m.groups.map((g) => g.key)).toEqual(["repo:git:b", OTHER_KEY]);
    expect(m.groups[1]).toMatchObject({ total: 12, filter: null, colorKey: null });
  });

  it("gives groups without a stable key no filter, and real groups the filter that isolates them", () => {
    const m = dailyMatrix(s, all, "repo");
    expect(m.groups.find((g) => g.label === "(no repo)")?.filter).toBeNull();
    expect(m.groups[0].filter).toEqual({ repos: ["git:b"] });
  });
});

describe("smoothDaily", () => {
  it("averages a centred window and only over the days that exist at the edges", () => {
    const m = { days: 5, groups: [{ key: "k", label: "k", colorKey: "k", filter: null, total: 9 }], values: Float64Array.from([9, 0, 0, 0, 3]) };
    expect([...smoothDaily(m, 3).values]).toEqual([4.5, 3, 0, 1, 1.5]);
  });
});

describe("snapshot.slots (core) as the chronos views consume it", () => {
  beforeEach(resetColorSlots);
  const community = (size: number) => ({ label: "x", blurb: "", tags: [], size, centroid_xy: [0, 0] as [number, number] });
  const models = [{ key: "m-a", label: "a" }, { key: "m-b", label: "b" }];

  it("ranks first sighting by snapshot-wide counts, then keeps colours across a swap that flips sizes and reorders meta", () => {
    const first = snapshot([conv({ repo: 0, models: [1] }), conv({ repo: 0, models: [1] }), conv({ repo: 1, models: [0, 1] })], {
      repos: [repos[0], repos[1]],
      models,
      communities: [community(17), community(19)],
    }).slots;
    expect(first.repo).toEqual(new Map([["git:a", 0], ["git:b", 1]]));
    expect(first.model).toEqual(new Map([["m-b", 0], ["m-a", 1]]));
    expect(first.community).toEqual(new Map([["1", 0], ["0", 1]]));

    // next snapshot: b outgrew a, a new repo c is listed first, community 0 overtook 1
    const next = snapshot([conv({ repo: 0 }), conv({ repo: 1 }), conv({ repo: 1 }), conv({ repo: 1 }), conv({ repo: 2 })], {
      repos: [repos[2], repos[1], repos[0]],
      models,
      communities: [community(30), community(19), community(4)],
    }).slots;
    expect(next.repo).toEqual(new Map([["git:c", 2], ["git:b", 1], ["git:a", 0]]));
    expect(next.community).toEqual(new Map([["0", 1], ["1", 0], ["2", 2]]));
  });
});

describe("stackLayers", () => {
  const s = snapshot(
    [conv({ repo: 0, activity: [[0, 3], [1, 1]] }), conv({ repo: 1, activity: [[0, 2], [1, 6]] })],
    { repos, days: 2 },
  );
  const m = dailyMatrix(s, all, "repo");

  it("stacked: the top of the last layer is the day's total", () => {
    const layers = stackLayers(m, "stacked");
    expect(layers[layers.length - 1].map((p) => p[1])).toEqual([5, 7]);
  });

  it("stream: every layer's thickness is its group's value, whatever the baseline", () => {
    for (const layer of stackLayers(m, "stream")) {
      layer.forEach((p, d) => expect(p[1] - p[0]).toBeCloseTo(m.values[d * m.groups.length + layer.key]));
    }
  });
});

describe("filterIsActive", () => {
  it("is true only when every key of the patch matches the current filters exactly", () => {
    const f = { ...NO_FILTERS, repos: ["git:a"] };
    expect(filterIsActive(f, { repos: ["git:a"] })).toBe(true);
    expect(filterIsActive({ ...f, repos: ["git:a", "git:b"] }, { repos: ["git:a"] })).toBe(false);
    expect(filterIsActive(f, null)).toBe(false);
  });
});

const turn = (n: number, t_ms: number): SessionTurn => ({ n, t_ms, role: "agent", source: "system", label: "", tags: [], xy: null });

describe("shiftMarkers", () => {
  // shipped turns are sparse by number: 2, 5, 9, 20
  const turns = [turn(2, 100), turn(5, 200), turn(9, 300), turn(20, 400)];
  const shift = (n: number, label = `s${n}`) => ({ n, t_ms: 0, score: 1, label, tags: [] });
  const markers = (shifts: Session["shifts"]) =>
    shiftMarkers({ guid: "g", turns, novelty: { window: 25, series: [] }, shifts }).map((m) => [m.n, m.idx]);

  it("lands a shift on its exact turn", () => {
    expect(markers([shift(9)])).toEqual([[9, 2]]);
  });

  it("lands a shift between shipped turns on the nearest by number, the earlier one on a tie", () => {
    expect(markers([shift(6), shift(8), shift(7)])).toEqual([
      [6, 1],
      [7, 1],
      [8, 2],
    ]);
  });

  it("drops shifts outside the shipped turn span instead of pinning them to an edge", () => {
    expect(markers([shift(1), shift(21), shift(20)])).toEqual([[20, 3]]);
  });
});

describe("labelLanes", () => {
  it("keeps non-overlapping labels in lane 0 and pushes overlaps to the first free lane", () => {
    const lanes = labelLanes([
      { x: 0, width: 50 },
      { x: 20, width: 50 }, // overlaps the first
      { x: 40, width: 10 }, // overlaps both
      { x: 80, width: 10 }, // lane 1 ended at 70, lane 0 at 50: back to lane 0
    ]);
    expect(lanes).toEqual([0, 1, 2, 0]);
  });
});

describe("turnIndexAt", () => {
  const turns = [turn(1, 100), turn(2, 200), turn(3, 200), turn(4, 300)];
  it("is the last turn at or before the cursor, including every turn sharing a timestamp", () => {
    expect(turnIndexAt(turns, 99)).toBe(-1);
    expect(turnIndexAt(turns, 100)).toBe(0);
    expect(turnIndexAt(turns, 250)).toBe(2);
    expect(turnIndexAt(turns, 9e9)).toBe(3);
  });
});

const counts = (c: Partial<Record<"work" | "tool_call" | "tool_result" | "retrieval" | "human" | "peer" | "agent", number>>) => ({
  work: 0,
  tool_call: 0,
  tool_result: 0,
  retrieval: 0,
  human: 0,
  peer: 0,
  agent: 0,
  ...c,
});

describe("autonomyIndex", () => {
  it("is human turns per 100 tool_call turns, and undefined (null) without tool calls", () => {
    expect(autonomyIndex({ human: 3, tool_call: 60 })).toBe(5);
    expect(autonomyIndex({ human: 0, tool_call: 10 })).toBe(0);
    expect(autonomyIndex({ human: 4, tool_call: 0 })).toBeNull();
  });

  it("does not move with which role a harness gives tool results (omp: agent, claude: human)", () => {
    // same session: 2 typed prompts, 40 tool calls, 40 tool results. n_human is source=human in both;
    // only n_agent (role=agent) differs, and the old human-per-agent ratio split them 2x.
    const omp = counts({ tool_call: 40, tool_result: 40, human: 2, agent: 80 });
    const claude = counts({ tool_call: 40, tool_result: 40, human: 2, agent: 40 });
    expect(autonomyIndex(omp)).toBe(5);
    expect(autonomyIndex(claude)).toBe(5);
  });
});

describe("compositionRows at the playhead", () => {
  const s = snapshot(
    [
      // 40 turns total: 10 on day 0, 30 on day 2; final mix work 1 : tool_call 3
      conv({ started_ms: DAY0, activity: [[0, 10], [2, 30]], counts: counts({ work: 10, tool_call: 30, human: 2, agent: 8 }) }),
      conv({ started_ms: DAY0 + 3 * DAY_MS, activity: [[3, 5]], counts: counts({ work: 5, agent: 5 }) }),
    ],
    { days: 5 },
  );

  it("scales the final mix to the turns that have happened by the playhead's day", () => {
    const [row] = compositionRows(s, () => true, DAY0 + 1 * DAY_MS + 5);
    expect(row.turnsNow).toBe(10);
    expect(row.byClass).toEqual([2.5, 7.5, 0, 0]);
    expect(row.byOrigin).toEqual([2, 0, 8]);
  });

  it("omits conversations that have not started and those the filters reject", () => {
    expect(compositionRows(s, () => true, DAY0 + 2 * DAY_MS).map((r) => r.i)).toEqual([0]);
    expect(compositionRows(s, (i) => i === 1, DAY0 + 4 * DAY_MS).map((r) => r.i)).toEqual([1]);
  });

  it("gives an all-zero mix rather than NaN when a class family has no counts", () => {
    expect(scaleMix(counts({}), ["work", "tool_call"], 10)).toEqual([0, 0]);
  });
});

describe("sortRows", () => {
  const s = snapshot([
    conv({ label: "b", activity: [[0, 5]], counts: counts({ human: 1, tool_call: 10, agent: 1000 }) }),
    conv({ label: "a", activity: [[0, 5]], counts: counts({ human: 3, tool_call: 0, agent: 50 }) }),
    conv({ label: "c", activity: [[0, 9]], counts: counts({ human: 1, tool_call: 100 }) }),
  ]);
  const rows = compositionRows(s, () => true, DAY0 + DAY_MS);
  const order = (by: CompositionSort) => sortRows(rows, s, by).map((r) => s.conversations[r.i].label);

  it("sorts autonomy by human per tool call, most autonomous first, and puts undefined autonomy last", () => {
    expect(order("autonomy")).toEqual(["c", "b", "a"]);
  });

  it("breaks ties by snapshot order so rows do not shuffle as the playhead moves", () => {
    expect(order("turns")).toEqual(["c", "b", "a"]);
  });
});

describe("facetRows", () => {
  it("aggregates autonomy from summed counts, not by averaging per-conversation ratios", () => {
    const s = snapshot(
      [
        conv({ repo: 0, activity: [[0, 2]], counts: counts({ human: 1, tool_call: 1 }) }), // 100
        conv({ repo: 0, activity: [[0, 99]], counts: counts({ human: 0, tool_call: 99 }) }), // 0
      ],
      { repos },
    );
    const [f] = facetRows(compositionRows(s, () => true, DAY0 + DAY_MS), s, "repo");
    expect(f.autonomy).toBe(1);
    expect(f.turnsNow).toBe(101);
    expect(f.filter).toEqual({ repos: ["git:a"] });
  });

  it("'all' gives the fleet autonomy as Σ raw human / Σ raw tool_call, not from the turn-scaled mixes", () => {
    const s = snapshot([
      conv({ activity: [[0, 2]], counts: counts({ human: 1, tool_call: 1 }) }),
      // tool results halve this conversation's scaled tool_call share; the raw sum must not care
      conv({ activity: [[0, 99]], counts: counts({ human: 0, tool_call: 99, tool_result: 99, agent: 500 }) }),
    ]);
    const rows = compositionRows(s, () => true, DAY0 + DAY_MS);
    const [fleet, ...rest] = facetRows(rows, s, "all");
    expect(rest).toEqual([]);
    expect(fleet.rows).toHaveLength(2);
    expect(fleet.autonomy).toBe(1); // 1 / 100 tool calls; agent turns play no part
  });
});

describe("tag matrices", () => {
  const vocab = ["x1", "y1", "x2", "y2", "x3"].map((tag) => ({ tag, n: 0, conversations: 0 }));
  const s = snapshot(
    [
      conv({ tags: ["x1", "x2", "x3"] }),
      conv({ tags: ["x1", "x2", "x3", "unknown"] }),
      conv({ tags: ["y1", "y2"] }),
      conv({ tags: ["y1", "y2", "x1"], started_ms: DAY0 + 5 * DAY_MS }),
      conv({ tags: ["x1", "x1"] }), // duplicates count once
    ],
    {},
    { tags: vocab, cooc: [] },
  );
  const cell = (m: TagMatrix, a: string, b: string) =>
    m.cooc[m.tags.indexOf(a) * m.tags.length + m.tags.indexOf(b)];

  it("recomputes conversation-level counts over conversations that pass and exist at the playhead", () => {
    const before = recomputedTagMatrix(s, () => true, DAY0, 10);
    expect(before.conversations).toBe(4);
    expect(before.tags[0]).toBe("x1");
    expect(before.n[0]).toBe(3);
    expect(cell(before, "x1", "x2")).toBe(2);
    expect(cell(before, "y1", "x1")).toBe(0);
    const after = recomputedTagMatrix(s, () => true, DAY0 + 9 * DAY_MS, 10);
    expect(cell(after, "y1", "x1")).toBe(1);
    expect(cell(after, "x1", "y1")).toBe(1); // symmetric
  });

  it("keeps the topN most common tags, ignoring tags outside the snapshot vocabulary", () => {
    const m = recomputedTagMatrix(s, () => true, DAY0 + 9 * DAY_MS, 2);
    expect(m.tags).toEqual(["x1", "x2"]);
    expect(recomputedTagMatrix(s, () => true, DAY0, 10).tags).not.toContain("unknown");
  });

  it("reads the snapshot's cooc pairs into a symmetric matrix", () => {
    const snap = snapshot([], {}, { tags: vocab.map((t, i) => ({ ...t, conversations: 10 - i })), cooc: [[0, 2, 7]] });
    const m = snapshotTagMatrix(snap, 5);
    expect(cell(m, "x1", "x2")).toBe(7);
    expect(cell(m, "x2", "x1")).toBe(7);
    expect(cell(m, "x1", "x1")).toBe(10);
  });

  it("clustered order puts tags that travel together next to each other", () => {
    const m = recomputedTagMatrix(s, () => true, DAY0, 10); // frequency order interleaves x and y
    const order = clusterOrder(jaccard(m), m.tags.length).map((k) => m.tags[k][0]);
    expect(order.join("")).toMatch(/^(x+y+|y+x+)$/);
  });
});

describe("overview", () => {
  const seat = (id: string, day: number) => ({ id, harness: null, conversations: [], first_ms: DAY0 + day * DAY_MS, last_ms: DAY0 + 9 * DAY_MS });
  const s = snapshot(
    [
      conv({ seat: 0, community: 0, harness: 0, activity: [[0, 5], [3, 5]] }),
      conv({ seat: 1, community: 0, harness: 1, started_ms: DAY0 + 2 * DAY_MS, activity: [[2, 4]] }),
      conv({ kind: "subagent", models: [1, 0], activity: [[0, 1]] }),
      conv({ seat: 3, community: 1, harness: 0, started_ms: DAY0 + 6 * DAY_MS, activity: [[6, 2]] }),
    ],
    // s2 owns no conversation (a message-only seat)
    { harnesses: ["omp", "claude"], seats: [seat("s0", 0), seat("s1", 2), seat("s2", 1), seat("s3", 6)] },
    undefined,
    [
      { from: 0, to: 1, to_conv: 1, day: 1, n: 2 },
      { from: 1, to: 0, to_conv: 0, day: 2, n: 1 },
      { from: 2, to: 0, to_conv: 0, day: 1, n: 4 },
      { from: null, to: null, to_conv: 2, day: 2, n: 3 },
      { from: 0, to: 1, to_conv: 1, day: 5, n: 9 },
    ],
  );
  const day2 = DAY0 + 2 * DAY_MS + 1;

  it("counts what exists at the playhead, with turns only up to the playhead's day", () => {
    const o = overview(s, NO_FILTERS, day2);
    expect([o.conversations, o.subagents, o.turns]).toEqual([3, 1, 10]);
    expect(o.communities).toEqual([{ i: 0, n: 2 }]);
    expect(o.unclustered).toBe(1);
    expect(o.harnesses).toEqual([{ i: 0, n: 1 }, { i: 1, n: 1 }, { i: null, n: 1 }]); // ties: index order, unknown last
    expect(o.models).toEqual([{ i: 0, n: 1 }, { i: 1, n: 1 }]);
    expect(Array.from(o.daily)).toEqual([6, 0, 4, 5, 0, 0, 2, 0, 0, 0]); // growth is not clipped to the playhead
  });

  it("counts seats that exist by the playhead, whether or not they own a conversation", () => {
    const o = overview(s, NO_FILTERS, day2);
    expect([o.seats, o.seatsWithConversation]).toEqual([3, 2]);
    expect(overview(s, { ...NO_FILTERS, seats: ["s2"] }, day2).seats).toBe(1);
  });

  it("tallies every message through the playhead's day, message-only senders and unknown endpoints included", () => {
    expect(overview(s, NO_FILTERS, day2).seatMessages).toEqual([
      { i: 0, sent: 2, received: 5 },
      { i: null, sent: 3, received: 3 },
      { i: 2, sent: 4, received: 0 },
      { i: 1, sent: 1, received: 2 },
    ]);
  });

  it("attributes messages to filters through the receiving conversation", () => {
    const omp = overview(s, { ...NO_FILTERS, harnesses: ["omp"] }, day2);
    expect(omp.seatMessages).toEqual([
      { i: 0, sent: 0, received: 5 },
      { i: 2, sent: 4, received: 0 },
      { i: 1, sent: 1, received: 0 },
    ]);
    expect(omp.communities).toEqual([{ i: 0, n: 1 }]);
  });

  it("applies the seat filter to message endpoints, not to the seats that own a conversation", () => {
    const s2 = overview(s, { ...NO_FILTERS, seats: ["s2"] }, day2);
    expect(s2.conversations).toBe(0);
    expect(s2.seatMessages).toEqual([
      { i: 0, sent: 0, received: 4 },
      { i: 2, sent: 4, received: 0 },
    ]);
  });
});

describe("throughDay", () => {
  it("names the playhead's UTC day, never a day after the newest turn", () => {
    const s = snapshot([], { end_ms: DAY0 + 3 * DAY_MS + 5_000 });
    expect(throughDay(s, DAY0 + DAY_MS + 1)).toBe("through 2 Aug 2026 (UTC)");
    expect(throughDay(s, DAY0 + 4 * DAY_MS)).toBe("through 4 Aug 2026 (UTC)"); // clamped to end_ms
  });
});

describe("caveats", () => {
  it("says 'not recorded' (null text) for keys the snapshot did not emit", () => {
    expect(caveats({}).map((c) => [c.text, c.warn])).toEqual([
      [null, false],
      [null, false],
      [null, false],
      [null, false],
    ]);
  });

  it("warns on low coverage, a harness share above chance, and any DL-029 turns", () => {
    const [cov, harness, dl029] = caveats({
      coverage_summary: 0.85,
      coverage_any_vector: 0.97,
      harness_confound_same_harness_edge_share: 0.85,
      harness_confound_random_baseline: 0.5,
      dl029_tool_call_turns: 2601,
    });
    expect(cov).toMatchObject({ warn: true, text: expect.stringContaining("85% of work turns are summarised; 97% have a vector") });
    expect(harness).toMatchObject({ warn: true, text: expect.stringContaining("85% of kNN edges join the same harness, against 50% by chance") });
    expect(dl029).toMatchObject({ warn: true, text: expect.stringContaining("2,601 tool_call turns") });
  });

  it("warns that omp human/peer figures are unreliable when omp sessions have no human turns", () => {
    const omp = caveats({ omp_sessions_zero_human: 126 })[3];
    expect(omp).toMatchObject({ title: "omp human/peer attribution", warn: true });
    expect(omp.text).toContain("126 omp sessions have no human turns");
    expect(omp.text).toContain("human, peer and autonomy figures for omp are unreliable");
    expect(caveats({ omp_sessions_zero_human: 0 })[3].warn).toBe(false);
  });

  it("does not warn when the numbers are fine", () => {
    const warns = caveats({
      coverage_summary: 0.95,
      harness_confound_same_harness_edge_share: 0.49,
      harness_confound_random_baseline: 0.51,
      dl029_tool_call_turns: 0,
      omp_sessions_zero_human: 0,
    }).map((c) => c.warn);
    expect(warns).toEqual([false, false, false, false]);
  });
});
