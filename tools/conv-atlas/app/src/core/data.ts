/**
 * Data access: the only module that fetches. Everything is relative to DATA_BASE,
 * which the dev server and `conv-atlas serve` both map to $CONV_ATLAS_HOME:
 *   <base>/bundles/manifest.json, <base>/bundles/<id>/view/*.json, <base>/insights/*.json
 */
import type {
  Conversation,
  Edges,
  Insight,
  InsightIndex,
  Manifest,
  Meta,
  Session,
  Tags,
} from "../contract/types";
import { SCHEMA_VERSION } from "../contract/types";
import { type SnapshotSlots, snapshotSlots } from "./colorSlots";

export const DATA_BASE: string = import.meta.env.VITE_DATA_BASE ?? "/data";

export interface LoadedSnapshot {
  id: string;
  meta: Meta;
  conversations: Conversation[];
  edges: Edges;
  tags: Tags;
  /** guid -> conversation index */
  guidIndex: Map<string, number>;
  /** per conversation: prefix sums of activity turns, aligned with conversation.activity */
  activityPrefix: Float64Array[];
  /** sticky colour slots for repo / community / model keys; every view colours by these */
  slots: SnapshotSlots;
}

async function getJson<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${DATA_BASE}/${path}`, { cache: "no-store", ...init });
  if (!res.ok) throw new Error(`GET ${path}: ${res.status} ${res.statusText}`);
  return (await res.json()) as T;
}

export async function loadManifest(): Promise<Manifest> {
  const m = await getJson<Manifest>("bundles/manifest.json");
  if (m.schema_version !== SCHEMA_VERSION) {
    throw new Error(`manifest schema_version ${m.schema_version} is not supported (app speaks ${SCHEMA_VERSION})`);
  }
  return m;
}

export async function loadSnapshot(id: string): Promise<LoadedSnapshot> {
  const base = `bundles/${id}/view`;
  const [meta, conversations, edges, tags] = await Promise.all([
    getJson<Meta>(`${base}/meta.json`),
    getJson<Conversation[]>(`${base}/conversations.json`),
    getJson<Edges>(`${base}/edges.json`),
    getJson<Tags>(`${base}/tags.json`),
  ]);
  if (meta.schema_version !== SCHEMA_VERSION) {
    throw new Error(`snapshot ${id} schema_version ${meta.schema_version} is not supported`);
  }
  return buildSnapshot(id, meta, conversations, edges, tags);
}

/** Derive the indices every view relies on. Test kits build snapshots through this too. */
export function buildSnapshot(
  id: string,
  meta: Meta,
  conversations: Conversation[],
  edges: Edges,
  tags: Tags,
): LoadedSnapshot {
  const guidIndex = new Map(conversations.map((c, i) => [c.guid, i]));
  const activityPrefix = conversations.map((c) => {
    const p = new Float64Array(c.activity.length);
    let sum = 0;
    c.activity.forEach(([, n], i) => {
      sum += n;
      p[i] = sum;
    });
    return p;
  });
  return { id, meta, conversations, edges, tags, guidIndex, activityPrefix, slots: snapshotSlots(meta, conversations) };
}

const sessionCache = new Map<string, Promise<Session>>();
const SESSION_CACHE_MAX = 24;

/** Lazy per-session detail; only conversations with session_detail=true have one. */
export function loadSession(snapshotId: string, guid: string): Promise<Session> {
  const key = `${snapshotId}/${guid}`;
  let hit = sessionCache.get(key);
  if (hit) {
    sessionCache.delete(key);
    sessionCache.set(key, hit);
    return hit;
  }
  hit = getJson<Session>(`bundles/${snapshotId}/view/sessions/${guid}.json`);
  hit.catch(() => sessionCache.delete(key));
  sessionCache.set(key, hit);
  if (sessionCache.size > SESSION_CACHE_MAX) {
    sessionCache.delete(sessionCache.keys().next().value as string);
  }
  return hit;
}

/** Insights live outside snapshots; a missing index means "none yet", not an error. */
export async function loadInsightIndex(): Promise<InsightIndex> {
  try {
    return await getJson<InsightIndex>("insights/index.json");
  } catch {
    return { schema_version: SCHEMA_VERSION, insights: [] };
  }
}

export function loadInsight(id: string): Promise<Insight> {
  return getJson<Insight>(`insights/${id}.json`);
}
