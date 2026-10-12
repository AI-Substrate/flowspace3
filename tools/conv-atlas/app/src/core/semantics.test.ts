import { describe, expect, it } from "vitest";
import type { Conversation, Meta } from "../contract/types";
import { buildSnapshot, type LoadedSnapshot } from "./data";
import { DAY_MS, NO_FILTERS, turnsUpTo, visibility } from "./semantics";

const DAY0 = Date.UTC(2026, 7, 1);

function conv(over: Partial<Conversation>): Conversation {
  return {
    guid: crypto.randomUUID(),
    kind: "session",
    label: "x",
    blurb: null,
    repo: null,
    harness: null,
    seat: null,
    models: [],
    parent: null,
    community: null,
    xy: [0, 0],
    started_ms: DAY0,
    last_ms: DAY0,
    turns: 0,
    counts: { work: 0, tool_call: 0, tool_result: 0, retrieval: 0, human: 0, peer: 0, agent: 0 },
    tags: [],
    activity: [],
    session_detail: false,
    ...over,
  };
}

function snapshot(conversations: Conversation[], meta: Partial<Meta> = {}): LoadedSnapshot {
  return buildSnapshot(
    "t",
    { day0_ms: DAY0, days: 30, repos: [], harnesses: [], models: [], seats: [], communities: [], ...meta } as Meta,
    conversations,
    { knn: { k: 1, edges: [] }, subagent: [], messages: [] },
    { tags: [], cooc: [] },
  );
}

describe("turnsUpTo", () => {
  const s = snapshot([conv({ activity: [[2, 5], [4, 10], [9, 1]] })]);
  it("counts turns through the end of the given day, inclusive", () => {
    expect(turnsUpTo(s, 0, 1)).toBe(0);
    expect(turnsUpTo(s, 0, 2)).toBe(5);
    expect(turnsUpTo(s, 0, 8)).toBe(15);
    expect(turnsUpTo(s, 0, 99)).toBe(16);
  });
});

describe("visibility", () => {
  const s = snapshot(
    [
      conv({ started_ms: DAY0, repo: 0 }),
      conv({ started_ms: DAY0 + 5 * DAY_MS, repo: 1 }),
      conv({ started_ms: DAY0 + 9 * DAY_MS, repo: null }),
    ],
    {
      repos: [
        { key: "git:a", label: "a" },
        { key: "git:b", label: "b" },
      ],
    },
  );
  it("hides conversations that have not started at the playhead", () => {
    expect([...visibility(s, NO_FILTERS, DAY0 + 5 * DAY_MS)]).toEqual([1, 1, 0]);
  });
  it("filters by stable repo key, and unknown repos never match a repo filter", () => {
    const f = { ...NO_FILTERS, repos: ["git:b"] };
    expect([...visibility(s, f, DAY0 + 30 * DAY_MS)]).toEqual([0, 1, 0]);
  });
});
