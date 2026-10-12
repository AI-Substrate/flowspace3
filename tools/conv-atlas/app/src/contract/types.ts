/**
 * View-layer types: a hand mirror of contract/bundle.schema.json (schema_version 1).
 * The schema is the authority. The exporter validates every view file against it
 * before publishing, so a field changed here must change there in the same commit.
 *
 * Conventions: times are UTC epoch ms; every cross-reference is an index into an
 * array of the same snapshot; `null` means unknown/not applicable.
 */

export const SCHEMA_VERSION = 1 as const;

export type Ms = number;
export type Idx = number;
export type XY = [number, number];
export type Source = "prod" | "synthetic";

export interface ManifestSnapshot {
  id: string;
  created_ms: Ms;
  source: Source;
  counts: { conversations: number; turns: number; seats: number; communities: number };
  time_range: { start_ms: Ms; end_ms: Ms };
}

export interface Manifest {
  schema_version: typeof SCHEMA_VERSION;
  latest: string;
  snapshots: ManifestSnapshot[];
}

export interface Community {
  label: string;
  blurb: string;
  tags: string[];
  size: number;
  centroid_xy: XY;
}

export interface Seat {
  id: string;
  harness: Idx | null;
  conversations: Idx[];
  /** earliest evidence of the seat */
  first_ms: Ms;
  /** latest evidence of the seat */
  last_ms: Ms;
}

export interface Meta {
  schema_version: typeof SCHEMA_VERSION;
  id: string;
  created_ms: Ms;
  source: Source;
  /** UTC midnight of day index 0 */
  day0_ms: Ms;
  days: number;
  /** earliest conversation start (exact) */
  start_ms: Ms;
  /** newest turn (exact): the true end of the data; playback ends here */
  end_ms: Ms;
  repos: { key: string; label: string }[];
  harnesses: string[];
  models: { key: string; label: string }[];
  seats: Seat[];
  communities: Community[];
  layout: { method: string; params?: Record<string, unknown>; bounds: [number, number, number, number] };
  stats: Record<string, number>;
}

export interface TurnCounts {
  work: number;
  tool_call: number;
  tool_result: number;
  retrieval: number;
  human: number;
  peer: number;
  agent: number;
}

export interface Conversation {
  guid: string;
  kind: "session" | "subagent";
  label: string;
  blurb: string | null;
  repo: Idx | null;
  harness: Idx | null;
  seat: Idx | null;
  models: Idx[];
  parent: Idx | null;
  /** null = unclustered */
  community: Idx | null;
  /** null = not embedded (too few work turns) */
  xy: XY | null;
  started_ms: Ms;
  last_ms: Ms;
  turns: number;
  counts: TurnCounts;
  tags: string[];
  /** sparse [day, turns], ascending by day */
  activity: [Idx, number][];
  session_detail: boolean;
}

/** [a, b, weight (centred cosine), t_ms], a < b */
export type KnnEdge = [Idx, Idx, number, Ms];
/** [parent, child, t_ms] */
export type SubagentEdge = [Idx, Idx, Ms];

export interface MessageEdge {
  /** sender seat; null = unknown sender */
  from: Idx | null;
  /** receiver seat; null = receiver seat unknown */
  to: Idx | null;
  /** receiving conversation (always set): filters attribute a message through it */
  to_conv: Idx;
  day: Idx;
  n: number;
}

export interface Edges {
  knn: { k: number; edges: KnnEdge[] };
  subagent: SubagentEdge[];
  messages: MessageEdge[];
}

export interface Tags {
  tags: { tag: string; n: number; conversations: number }[];
  /** [i, j, n] with i < j */
  cooc: [Idx, Idx, number][];
}

export interface SessionTurn {
  /** turn_no; fs3 address is conv:<guid>#t<n> */
  n: number;
  t_ms: Ms;
  role: "human" | "agent";
  source: "human" | "peer" | "system";
  label: string;
  tags: string[];
  xy: XY | null;
}

export interface Session {
  guid: string;
  turns: SessionTurn[];
  /** series: [index into turns, novelty 0..2] */
  novelty: { window: number; series: [Idx, number][] };
  shifts: { n: number; t_ms: Ms; score: number; label: string; tags: string[] }[];
}

export type InsightKind = "finding" | "question" | "anomaly" | "story";

export interface Insight {
  id: string;
  title: string;
  body_md: string;
  author: string;
  created_ms: Ms;
  snapshot_id: string;
  kind?: InsightKind;
  refs: {
    guids?: string[];
    addresses?: string[];
    seats?: string[];
    repos?: string[];
    tags?: string[];
    time_range?: { start_ms: Ms; end_ms: Ms };
  };
}

export interface InsightIndex {
  schema_version: typeof SCHEMA_VERSION;
  insights: { id: string; title: string; kind: InsightKind; created_ms: Ms }[];
}
