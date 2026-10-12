/** Synthetic snapshot builders for Chronos unit tests (test-only; no real data). */
import { buildSnapshot, type LoadedSnapshot } from "../../core/data";
import type { Conversation, MessageEdge, Meta, Tags } from "../../contract/types";

export const DAY0 = Date.UTC(2026, 7, 1);

let seq = 0;
export function conv(over: Partial<Conversation>): Conversation {
  return {
    guid: `g${seq++}`,
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

export function snapshot(
  conversations: Conversation[],
  meta: Partial<Meta> = {},
  tags?: Tags,
  messages: MessageEdge[] = [],
): LoadedSnapshot {
  return buildSnapshot(
    "t",
    { day0_ms: DAY0, days: 10, repos: [], harnesses: [], models: [], seats: [], communities: [], ...meta } as Meta,
    conversations,
    { knn: { k: 1, edges: [] }, subagent: [], messages },
    tags ?? { tags: [], cooc: [] },
  );
}
