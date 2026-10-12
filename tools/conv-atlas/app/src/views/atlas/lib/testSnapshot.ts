/** Synthetic snapshots for the atlas unit tests (no network, no fixture files). */
import type { Conversation, Meta } from "../../../contract/types";
import { buildSnapshot, type LoadedSnapshot } from "../../../core/data";

export const DAY0 = Date.UTC(2026, 7, 1);

export function conv(over: Partial<Conversation>): Conversation {
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

export function snapshot(conversations: Conversation[], meta: Partial<Meta> = {}): LoadedSnapshot {
  return buildSnapshot(
    "t",
    {
      day0_ms: DAY0,
      days: 30,
      repos: [],
      harnesses: [],
      models: [],
      seats: [],
      communities: [],
      layout: { method: "test", bounds: [-1, -1, 1, 1] },
      ...meta,
    } as Meta,
    conversations,
    { knn: { k: 1, edges: [] }, subagent: [], messages: [] },
    { tags: [], cooc: [] },
  );
}
