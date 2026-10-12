/**
 * The one shared store. Views read it with `useAtlas(selector)` (React) or
 * `atlas.subscribe(selector, listener)` (imperative, for 60 fps WebGL work).
 *
 * Stable identity rules:
 * - selection holds GUIDS, filters hold stable keys (see semantics.ts), so both
 *   survive a snapshot swap; community filters are snapshot-local and cleared.
 * - `previous` keeps the outgoing snapshot after a swap so views can tween by guid.
 */
import { useStore } from "zustand";
import { subscribeWithSelector } from "zustand/middleware";
import { createStore } from "zustand/vanilla";
import type { Insight, InsightIndex, Manifest, ManifestSnapshot, Ms } from "../contract/types";
import { type LoadedSnapshot, loadInsight, loadSnapshot } from "./data";
import { type Filters, fullRange as rangeOf, NO_FILTERS } from "./semantics";

export type ColorBy = "community" | "repo" | "harness" | "kind" | "age";

export interface TimeState {
  cursorMs: Ms;
  playing: boolean;
  /** simulated days per real second */
  speed: number;
  /** playback window; defaults to the snapshot's full time range */
  range: [Ms, Ms];
  loop: boolean;
}

export interface AtlasState {
  manifest: Manifest | null;
  snapshot: LoadedSnapshot | null;
  previous: LoadedSnapshot | null;
  /** a newer snapshot announced by the manifest, not yet accepted */
  pending: ManifestSnapshot | null;
  loading: boolean;
  error: string | null;
  insights: InsightIndex | null;
  /** insight id whose refs are currently highlighted */
  activeInsight: string | null;
  /** the loaded document for activeInsight (null while loading or when none) */
  insight: Insight | null;

  view: string;
  colorBy: ColorBy;
  filters: Filters;
  /** selected conversation guid */
  selected: string | null;
  /** hovered conversation guid */
  hovered: string | null;
  /** lasso / multi-selection, guids */
  picked: string[];
  /** a specific turn to reveal in the reader (river, search hits, insights) */
  focusTurn: { guid: string; n: number } | null;
  time: TimeState;

  setView(view: string): void;
  setColorBy(c: ColorBy): void;
  setFilters(patch: Partial<Filters>): void;
  select(guid: string | null): void;
  /** select the conversation AND ask the reader to reveal turn n */
  openTurn(guid: string, n: number): void;
  hover(guid: string | null): void;
  pick(guids: string[]): void;
  setTime(patch: Partial<TimeState>): void;
  /** jump the playhead to the end of the range and stop: "show everything" */
  showAll(): void;
  setManifest(m: Manifest): void;
  setInsights(i: InsightIndex): void;
  /** highlight an insight; core loads its document into `insight` */
  setActiveInsight(id: string | null): void;
  /** load a snapshot and make it current; keeps filters/selection/cursor where meaningful */
  openSnapshot(id: string): Promise<void>;
  acceptPending(): Promise<void>;
}

export const atlas = createStore<AtlasState>()(
  subscribeWithSelector((set, get) => ({
    manifest: null,
    snapshot: null,
    previous: null,
    pending: null,
    loading: false,
    error: null,
    insights: null,
    activeInsight: null,
    insight: null,

    view: "galaxy",
    colorBy: "community",
    filters: NO_FILTERS,
    selected: null,
    hovered: null,
    picked: [],
    focusTurn: null,
    time: { cursorMs: 0, playing: false, speed: 7, range: [0, 0], loop: false },

    setView: (view) => set({ view }),
    setColorBy: (colorBy) => set({ colorBy }),
    setFilters: (patch) => set({ filters: { ...get().filters, ...patch } }),
    select: (selected) => set({ selected, focusTurn: null }),
    openTurn: (guid, n) => set({ selected: guid, focusTurn: { guid, n } }),
    hover: (hovered) => set({ hovered }),
    pick: (picked) => set({ picked }),
    setTime: (patch) => set({ time: { ...get().time, ...patch } }),
    showAll: () => set({ time: { ...get().time, cursorMs: get().time.range[1], playing: false } }),
    setManifest: (manifest) => {
      const current = get().snapshot?.id;
      const latest = manifest.snapshots.find((s) => s.id === manifest.latest) ?? null;
      set({ manifest, pending: current && latest && latest.id !== current ? latest : null });
    },
    setInsights: (insights) => set({ insights }),
    setActiveInsight: (activeInsight) => {
      set({ activeInsight, insight: null });
      if (activeInsight === null) return;
      loadInsight(activeInsight).then(
        (doc) => {
          if (get().activeInsight === activeInsight) set({ insight: doc });
        },
        (e) => set({ error: `insight ${activeInsight}: ${e instanceof Error ? e.message : String(e)}` }),
      );
    },

    openSnapshot: async (id) => {
      set({ loading: true, error: null });
      try {
        const next = await loadSnapshot(id);
        const prev = get().snapshot;
        const fullRange = rangeOf(next.meta);
        const t = get().time;
        // first load: everything visible; later swaps keep the playhead unless it was parked at the end
        const atEnd = !prev || t.cursorMs >= t.range[1];
        set({
          snapshot: next,
          previous: prev,
          pending: null,
          loading: false,
          filters: { ...get().filters, communities: [] },
          selected: get().selected && next.guidIndex.has(get().selected as string) ? get().selected : null,
          picked: get().picked.filter((g) => next.guidIndex.has(g)),
          time: { ...t, range: fullRange, cursorMs: atEnd ? fullRange[1] : Math.min(t.cursorMs, fullRange[1]) },
        });
      } catch (e) {
        set({ loading: false, error: e instanceof Error ? e.message : String(e) });
      }
    },
    acceptPending: async () => {
      const p = get().pending;
      if (p) await get().openSnapshot(p.id);
    },
  })),
);

export function useAtlas<T>(selector: (s: AtlasState) => T): T {
  return useStore(atlas, selector);
}
