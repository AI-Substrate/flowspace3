/**
 * Imperative similarity-graph renderer (sigma + graphology). React mounts it
 * once; it listens to the store directly. Playback gates nodes (started at the
 * playhead) and edges (born at the playhead, both ends visible) through sigma
 * reducers, refreshed only when the gated sets or the playhead day change.
 * ForceAtlas2 runs in a web worker for a bounded time after each (re)build.
 * Links fade with their similarity rank so the strong structure reads through
 * the weak bulk; node labels go through the same collision placement as the
 * galaxy's cluster labels instead of sigma's coarse grid.
 */
import Graph from "graphology";
import Sigma from "sigma";
import type { EdgeDisplayData, NodeDisplayData } from "sigma/types";
import type { InsightKind, Ms } from "../../../contract/types";
import type { LoadedSnapshot } from "../../../core/data";
import { dayOf, turnsUpTo } from "../../../core/semantics";
import { type AtlasState, atlas } from "../../../core/store";
import { accent, chrome, css, dimmed, kindColor, neutral } from "../../../design/palette";
import { getTheme, onThemeChange } from "../../../design/theme";
import { type ColorCoding, colorCoding, type LegendEntry, recountLegend } from "../lib/coloring";
import { countAtOrBefore, PlayheadMask, passMask } from "../lib/gate";
import { type SimGraph, similarityGraph } from "../lib/graphModel";
import { runForceLayout } from "../forceLayout";
import { drawThemedNodeHover, placedLabelPainter } from "../sigmaTheme";

export interface GraphStats {
  nodes: number;
  edges: number;
  crossEdges: number;
  layingOut: boolean;
  /** the selection is filtered out or not started yet, so its neighbourhood is not focused */
  focusHidden: boolean;
}

export interface GraphCallbacks {
  tooltip(t: { guid: string; x: number; y: number } | null): void;
  legend(entries: LegendEntry[]): void;
  stats(s: GraphStats): void;
}

export interface GraphOptions {
  /** only cross-repo edges */
  bridgesOnly: boolean;
}

interface NodeAttrs {
  x: number;
  y: number;
  size: number;
  label: string;
  ci: number;
}

interface EdgeAttrs {
  weight: number;
  /** weight quantile in the snapshot, 0–1 */
  strength: number;
  rank: number;
  cross: boolean;
  a: number;
  b: number;
}

const MIN_SIZE = 2;
const MAX_SIZE = 14;
const LAYOUT_MS = 4000;
/** link brightness steps from weakest to strongest */
const EDGE_LEVELS = 8;
/** nodes smaller than this on screen get no label unless forced */
const LABEL_MIN_NODE_PX = 9;

/** Precomputed CSS colours for the reducers. Edge ramps run weakest → strongest. */
interface GraphInk {
  categories: string[];
  dimmed: string;
  accent: string;
  halo: string;
  cross: string[];
  same: string[];
  /** links outside the focused neighbourhood */
  unfocused: string;
}

export class GraphEngine {
  private readonly graph = new Graph<NodeAttrs, EdgeAttrs>({ type: "undirected", multi: false });
  private readonly sigma: Sigma<NodeAttrs, EdgeAttrs>;
  private snap: LoadedSnapshot | null = null;
  private sim: SimGraph | null = null;
  private gate: PlayheadMask | null = null;
  private color: ColorCoding | null = null;
  private day = 0;
  private edgeCount = 0;
  private maxSqrtTurns = 1;
  /** the selection and its neighbours, while selected */
  private focus: Set<string> | null = null;
  /** `focus` while the selection itself is visible; reducers dim against this */
  private liveFocus: Set<string> | null = null;
  private picked = new Set<string>();
  private halo: { guids: Set<string>; kind: InsightKind } = { guids: new Set(), kind: "finding" };
  private ink: GraphInk = { categories: [], dimmed: "", accent: "", halo: "", cross: [], same: [], unfocused: "" };
  /** stops the running ForceAtlas2 spell; null when idle */
  private stopLayoutFn: (() => void) | null = null;
  private lastStats = "";
  /** visible nodes per colour category, recounted with the stats */
  private legendCounts = new Int32Array(0);
  /** the colouring and counts behind the last legend sent to React */
  private lastLegend: { color: ColorCoding | null; counts: string } = { color: null, counts: "" };
  private readonly unsubs: (() => void)[] = [];

  constructor(
    private readonly container: HTMLElement,
    private readonly cb: GraphCallbacks,
    private options: GraphOptions,
  ) {
    const font = getComputedStyle(document.documentElement).getPropertyValue("--font-sans").trim() || "sans-serif";
    this.sigma = new Sigma(this.graph, container, {
      // labels go through placedLabelPainter after each render
      renderLabels: false,
      labelFont: font,
      labelSize: 12,
      labelWeight: "500",
      zIndex: true,
      defaultDrawNodeHover: drawThemedNodeHover,
      minCameraRatio: 0.03,
      maxCameraRatio: 4,
      nodeReducer: (key, data) => this.reduceNode(key, data),
      edgeReducer: (key, data) => this.reduceEdge(key, data),
    });
    this.sigma.on("afterRender", placedLabelPainter(this.sigma, LABEL_MIN_NODE_PX));
    this.applyTheme();
    this.wireSigma();
    this.wireStore();
    this.unsubs.push(
      onThemeChange(() => {
        this.applyTheme();
        this.recolor();
      }),
    );
  }

  destroy(): void {
    for (const u of this.unsubs) u();
    this.stopLayout();
    this.sigma.kill();
  }

  setOptions(options: GraphOptions): void {
    this.options = options;
    this.refresh();
  }

  /** Re-runs ForceAtlas2 from the current positions for another bounded spell. */
  relayout(): void {
    this.startLayout();
  }

  // ---------------------------------------------------------------- wiring

  private wireSigma(): void {
    const s = this.sigma;
    s.on("enterNode", ({ node, event }) => {
      if (!this.isVisible(node)) return;
      atlas.getState().hover(node);
      const rect = this.container.getBoundingClientRect();
      this.cb.tooltip({ guid: node, x: rect.left + event.x, y: rect.top + event.y });
    });
    s.on("leaveNode", () => {
      atlas.getState().hover(null);
      this.cb.tooltip(null);
    });
    s.on("clickNode", ({ node }) => {
      if (!this.isVisible(node)) return;
      const st = atlas.getState();
      st.pick([]);
      st.select(node);
    });
    s.on("clickStage", () => {
      const st = atlas.getState();
      st.select(null);
      st.pick([]);
    });
  }

  private wireStore(): void {
    const sub = <T>(sel: (s: AtlasState) => T, fn: (v: T) => void) => this.unsubs.push(atlas.subscribe(sel, fn));
    sub(
      (s) => s.snapshot,
      (snap) => {
        if (snap) this.open(snap);
      },
    );
    sub(
      (s) => s.filters,
      (f) => {
        if (!this.snap || !this.gate) return;
        this.gate.setPass(passMask(this.snap, f));
        this.refresh();
      },
    );
    sub(
      (s) => s.time.cursorMs,
      (cursor) => this.seek(cursor),
    );
    sub(
      (s) => s.colorBy,
      () => this.recolor(),
    );
    sub(
      (s) => s.selected,
      (guid) => {
        this.focus = guid && this.graph.hasNode(guid) ? new Set([guid, ...this.graph.neighbors(guid)]) : null;
        this.refresh();
      },
    );
    sub(
      (s) => s.picked,
      (picked) => {
        this.picked = new Set(picked);
        this.refresh();
      },
    );
    sub(
      (s) => s.hovered,
      () => this.refresh(),
    );
    sub(
      (s) => s.insight,
      (insight) => {
        const guids = new Set(insight?.refs.guids ?? []);
        for (const a of insight?.refs.addresses ?? []) {
          const m = /^conv:([^#]+)/.exec(a);
          if (m) guids.add(m[1]);
        }
        this.halo = { guids, kind: insight?.kind ?? "finding" };
        this.cacheInk();
        this.refresh();
      },
    );
    const snap = atlas.getState().snapshot;
    if (snap) this.open(snap);
  }

  // ------------------------------------------------------------- snapshot

  private open(snap: LoadedSnapshot): void {
    if (this.snap === snap) return;
    // keep settled positions so a new snapshot starts where the old one ended
    const previous = new Map<string, [number, number]>();
    this.graph.forEachNode((key, a) => {
      previous.set(key, [a.x, a.y]);
    });
    this.stopLayout();
    const st = atlas.getState();
    const sim = similarityGraph(snap, previous);
    let maxTurns = 1;
    for (const c of snap.conversations) maxTurns = Math.max(maxTurns, c.turns);
    this.snap = snap;
    this.sim = sim;
    this.maxSqrtTurns = Math.sqrt(maxTurns);
    this.gate = new PlayheadMask((i) => snap.conversations[i].started_ms, snap.conversations.length, passMask(snap, st.filters));
    this.gate.seek(st.time.cursorMs);
    this.day = dayOf(snap, st.time.cursorMs);
    this.edgeCount = countAtOrBefore(sim.order.times, st.time.cursorMs);
    this.color = colorCoding(snap, st.colorBy, getTheme());
    this.cacheInk();

    this.graph.clear();
    for (const n of sim.nodes) {
      this.graph.addNode(n.guid, { x: n.x, y: n.y, size: MIN_SIZE, label: snap.conversations[n.ci].label, ci: n.ci });
    }
    sim.edges.forEach((e, i) => {
      const a = snap.conversations[e.a].guid;
      const b = snap.conversations[e.b].guid;
      if (a !== b && !this.graph.hasEdge(a, b)) {
        this.graph.addEdgeWithKey(`e${i}`, a, b, { weight: e.weight, strength: e.strength, rank: e.rank, cross: e.cross, a: e.a, b: e.b });
      }
    });
    this.focus = st.selected && this.graph.hasNode(st.selected) ? new Set([st.selected, ...this.graph.neighbors(st.selected)]) : null;
    this.picked = new Set(st.picked);
    this.refresh();
    this.startLayout();
  }

  // ------------------------------------------------------------- playback

  private seek(cursor: Ms): void {
    if (!this.snap || !this.gate || !this.sim) return;
    const changed = this.gate.seek(cursor);
    const day = dayOf(this.snap, cursor);
    const edgeCount = countAtOrBefore(this.sim.order.times, cursor);
    if (changed || day !== this.day || edgeCount !== this.edgeCount) {
      this.day = day;
      this.edgeCount = edgeCount;
      this.refresh();
    }
  }

  private recolor(): void {
    if (!this.snap) return;
    this.color = colorCoding(this.snap, atlas.getState().colorBy, getTheme());
    this.cacheInk();
    this.refresh();
  }

  private applyTheme(): void {
    this.container.style.background = css(chrome("bg"));
    this.sigma.setSetting("defaultEdgeColor", css(neutral(0.25)));
    if (this.snap) this.color = colorCoding(this.snap, atlas.getState().colorBy, getTheme());
    this.cacheInk();
    this.refresh();
  }

  /** CSS strings for the reducers, rebuilt on colouring/theme/insight change rather than per node per frame. */
  private cacheInk(): void {
    // opaque mixes between background and ink (sigma's edge blending does not honour alpha);
    // the square keeps the weak bulk near the background while the top links stand out
    const ramp = (lo: number, hi: number) =>
      Array.from({ length: EDGE_LEVELS }, (_, k) => css(neutral(lo + (hi - lo) * (k / (EDGE_LEVELS - 1)) ** 2)));
    this.ink = {
      categories: this.color?.colors.map(css) ?? [],
      dimmed: css(dimmed()),
      accent: css(accent()),
      halo: css(kindColor(this.halo.kind)),
      cross: ramp(0.08, 0.6),
      same: ramp(0.035, 0.3),
      unfocused: css(neutral(0.05)),
    };
  }

  /** Coalesced: sigma re-runs the reducers on its next frame. */
  private refresh(): void {
    // a selection hidden by the filters or the playhead would dim the whole graph: show it unfocused instead
    const selected = atlas.getState().selected;
    this.liveFocus = this.focus && selected && this.isVisible(selected) ? this.focus : null;
    this.sigma.scheduleRefresh();
    this.emitStats();
  }

  // ------------------------------------------------------------- reducers

  private isVisible(guid: string): boolean {
    const ci = this.snap?.guidIndex.get(guid);
    return ci !== undefined && !!this.gate?.mask[ci];
  }

  private reduceNode(key: string, data: NodeAttrs): Partial<NodeDisplayData> {
    const { gate, snap, color } = this;
    if (!gate || !snap || !color) return { ...data, hidden: true };
    const ci = data.ci;
    if (!gate.exists[ci]) return { ...data, hidden: true };
    const st = atlas.getState();
    const out: Partial<NodeDisplayData> = { ...data };
    if (!gate.pass[ci]) {
      out.color = this.ink.dimmed;
      out.size = MIN_SIZE;
      out.label = null;
      out.zIndex = 0;
      return out;
    }
    out.size = MIN_SIZE + (MAX_SIZE - MIN_SIZE) * Math.min(1, Math.sqrt(turnsUpTo(snap, ci, this.day)) / this.maxSqrtTurns);
    out.color = this.ink.categories[color.category[ci]];
    out.zIndex = 1;
    if (this.liveFocus && !this.liveFocus.has(key)) {
      out.color = this.ink.dimmed;
      out.label = null;
      out.zIndex = 0;
    }
    if (this.halo.guids.has(key)) {
      out.color = this.ink.halo;
      out.highlighted = true;
      out.forceLabel = true;
      out.zIndex = 2;
    }
    if (key === st.selected || this.picked.has(key)) {
      out.color = this.ink.accent;
      out.highlighted = key === st.selected;
      out.forceLabel = key === st.selected;
      out.zIndex = 3;
    }
    if (key === st.hovered) out.highlighted = true;
    return out;
  }

  private reduceEdge(key: string, data: EdgeAttrs): Partial<EdgeDisplayData> {
    const { gate } = this;
    if (!gate || data.rank >= this.edgeCount || !gate.mask[data.a] || !gate.mask[data.b]) return { hidden: true };
    if (this.options.bridgesOnly && !data.cross) return { hidden: true };
    const focus = this.liveFocus;
    const focused = !focus || (focus.has(this.graph.source(key)) && focus.has(this.graph.target(key)));
    const level = Math.round(data.strength * (EDGE_LEVELS - 1));
    if (!focused) return { size: 0.4, color: this.ink.unfocused, zIndex: 0 };
    if (data.cross) return { size: 0.6 + 1.4 * data.strength, color: this.ink.cross[level], zIndex: EDGE_LEVELS + level };
    return { size: 0.3 + 0.8 * data.strength, color: this.ink.same[level], zIndex: level };
  }

  // ---------------------------------------------------------------- layout

  private startLayout(): void {
    this.stopLayout();
    this.stopLayoutFn = runForceLayout(this.graph, LAYOUT_MS, [this.container], () => {
      this.stopLayoutFn = null;
      this.emitStats();
    });
    this.emitStats();
  }

  private stopLayout(): void {
    this.stopLayoutFn?.();
  }

  private emitStats(): void {
    const { gate, sim, snap, color } = this;
    if (!gate || !sim || !snap || !color) return;
    if (this.legendCounts.length !== color.colors.length) this.legendCounts = new Int32Array(color.colors.length);
    else this.legendCounts.fill(0);
    let nodes = 0;
    for (const n of sim.nodes) {
      if (!gate.mask[n.ci]) continue;
      nodes++;
      this.legendCounts[color.category[n.ci]]++;
    }
    const counts = this.legendCounts.join();
    if (this.lastLegend.color !== color || this.lastLegend.counts !== counts) {
      this.lastLegend = { color, counts };
      this.cb.legend(recountLegend(color, this.legendCounts));
    }
    let edges = 0;
    let cross = 0;
    for (const e of sim.edges) {
      if (e.rank >= this.edgeCount || !gate.mask[e.a] || !gate.mask[e.b]) continue;
      edges++;
      if (e.cross) cross++;
    }
    const focusHidden = this.focus !== null && this.liveFocus === null;
    const stats: GraphStats = { nodes, edges, crossEdges: cross, layingOut: this.stopLayoutFn !== null, focusHidden };
    const key = JSON.stringify(stats);
    if (key === this.lastStats) return;
    this.lastStats = key;
    this.cb.stats(stats);
  }
}
