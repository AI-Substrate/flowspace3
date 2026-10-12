/**
 * Imperative seat-network renderer (sigma + graphology). Seats are nodes sized
 * by their conversations at the playhead; peer messages are directed links
 * aggregated up to the playhead day. Links carrying messages on the current day
 * "fire": a pulse travels along them, positioned by the time of day at the
 * playhead, drawn on an overlay canvas. Optional: subagent parent→child trees
 * hung off the parent's seat.
 */
import Graph from "graphology";
import Sigma from "sigma";
import type { EdgeDisplayData, NodeDisplayData } from "sigma/types";
import type { Ms } from "../../../contract/types";
import type { LoadedSnapshot } from "../../../core/data";
import { DAY_MS, dayOf } from "../../../core/semantics";
import { type AtlasState, atlas } from "../../../core/store";
import { accent, chrome, css, dimmed, harnessColor, neutral, withAlpha } from "../../../design/palette";
import { onThemeChange } from "../../../design/theme";
import { runForceLayout } from "../forceLayout";
import type { LegendEntry } from "../lib/coloring";
import { PlayheadMask, passMask } from "../lib/gate";
import { buildSeatNet, messagesOn, messagesThrough, type SeatNet } from "../lib/seatsModel";
import { drawThemedNodeHover, placedLabelPainter } from "../sigmaTheme";

export type SeatsAnchor = { kind: "seat"; seat: number; x: number; y: number } | { kind: "conv"; guid: string; x: number; y: number };

export interface SeatsStats {
  seats: number;
  links: number;
  /** messages so far that are drawn: at least one seat known */
  messages: number;
  /** of those, messages with both seats known */
  known: number;
  firing: number;
  /** messages so far with neither seat known: not drawn */
  unattributed: number;
  /** messages so far from a seat to itself: not drawn */
  selfSent: number;
}

export interface SeatsCallbacks {
  tooltip(t: SeatsAnchor | null): void;
  legend(entries: LegendEntry[]): void;
  stats(s: SeatsStats): void;
}

export interface SeatsOptions {
  trees: boolean;
}

interface NodeAttrs {
  x: number;
  y: number;
  size: number;
  label: string;
  /** seat index, or -1 for a conversation node */
  seat: number;
  /** conversation index, or -1 for a seat node */
  ci: number;
}

interface EdgeAttrs {
  weight: number;
  /** index into SeatNet.links, or -1 for tree edges */
  link: number;
  /** tree edges: the child conversation (or the parent, for seat→parent) that gates the edge */
  ci: number;
}

const LAYOUT_MS = 3000;
/** nodes smaller than this on screen get no label unless forced */
const LABEL_MIN_NODE_PX = 8;
const seatKey = (i: number) => `s:${i}`;
const convKey = (ci: number) => `c:${ci}`;

export class SeatsEngine {
  private readonly graph = new Graph<NodeAttrs, EdgeAttrs>({ type: "directed", multi: false });
  private readonly sigma: Sigma<NodeAttrs, EdgeAttrs>;
  private readonly ctx: CanvasRenderingContext2D;
  private snap: LoadedSnapshot | null = null;
  private net: SeatNet | null = null;
  private gate: PlayheadMask | null = null;
  /** per seat: visible conversations at the playhead */
  private seatVisible = new Int32Array(0);
  private day = 0;
  private cursor: Ms = 0;
  private stopLayoutFn: (() => void) | null = null;
  private lastStats = "";
  private readonly unsubs: (() => void)[] = [];

  constructor(
    private readonly container: HTMLElement,
    private readonly overlay: HTMLCanvasElement,
    private readonly cb: SeatsCallbacks,
    private options: SeatsOptions,
  ) {
    const ctx = overlay.getContext("2d");
    if (!ctx) throw new Error("seats: 2D canvas unavailable");
    this.ctx = ctx;
    const font = getComputedStyle(document.documentElement).getPropertyValue("--font-sans").trim() || "sans-serif";
    this.sigma = new Sigma(this.graph, container, {
      // labels go through placedLabelPainter after each render
      renderLabels: false,
      labelFont: font,
      labelSize: 12,
      labelWeight: "500",
      zIndex: true,
      defaultEdgeType: "arrow",
      defaultDrawNodeHover: drawThemedNodeHover,
      minCameraRatio: 0.05,
      maxCameraRatio: 4,
      nodeReducer: (key, data) => this.reduceNode(key, data),
      edgeReducer: (_key, data) => this.reduceEdge(data),
    });
    this.applyTheme();
    this.sigma.on("afterRender", placedLabelPainter(this.sigma, LABEL_MIN_NODE_PX));
    this.sigma.on("afterRender", () => this.paintPulses());
    this.wireSigma();
    this.wireStore();
    this.unsubs.push(onThemeChange(() => this.applyTheme()));
  }

  destroy(): void {
    for (const u of this.unsubs) u();
    this.stopLayoutFn?.();
    this.sigma.kill();
  }

  setOptions(options: SeatsOptions): void {
    const rebuild = options.trees !== this.options.trees;
    this.options = options;
    if (rebuild && this.snap) this.build(this.snap);
  }

  relayout(): void {
    this.startLayout();
  }

  // ---------------------------------------------------------------- wiring

  private wireSigma(): void {
    const s = this.sigma;
    s.on("enterNode", ({ node, event }) => {
      const a = this.graph.getNodeAttributes(node);
      const rect = this.container.getBoundingClientRect();
      const x = rect.left + event.x;
      const y = rect.top + event.y;
      if (a.seat >= 0) {
        // the unknown-sender node has no seat record to describe
        if (this.snap && a.seat < this.snap.meta.seats.length) this.cb.tooltip({ kind: "seat", seat: a.seat, x, y });
      } else if (this.snap) {
        const guid = this.snap.conversations[a.ci].guid;
        atlas.getState().hover(guid);
        this.cb.tooltip({ kind: "conv", guid, x, y });
      }
    });
    s.on("leaveNode", () => {
      atlas.getState().hover(null);
      this.cb.tooltip(null);
    });
    s.on("clickNode", ({ node }) => {
      const a = this.graph.getNodeAttributes(node);
      const st = atlas.getState();
      if (a.seat >= 0 && this.snap) {
        if (a.seat >= this.snap.meta.seats.length) return;
        // click a seat: filter to it; click it again: clear the seat filter
        const id = this.snap.meta.seats[a.seat].id;
        const only = st.filters.seats.length === 1 && st.filters.seats[0] === id;
        st.setFilters({ seats: only ? [] : [id] });
      } else if (this.snap) {
        st.select(this.snap.conversations[a.ci].guid);
      }
    });
  }

  private wireStore(): void {
    const sub = <T>(sel: (s: AtlasState) => T, fn: (v: T) => void) => this.unsubs.push(atlas.subscribe(sel, fn));
    sub(
      (s) => s.snapshot,
      (snap) => {
        if (snap) this.build(snap);
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
      (s) => s.selected,
      () => this.refresh(),
    );
    const snap = atlas.getState().snapshot;
    if (snap) this.build(snap);
  }

  // ------------------------------------------------------------- building

  private build(snap: LoadedSnapshot): void {
    const previous = new Map<string, [number, number]>();
    this.graph.forEachNode((key, a) => {
      previous.set(key, [a.x, a.y]);
    });
    this.stopLayoutFn?.();
    const st = atlas.getState();
    const net = buildSeatNet(snap);
    this.snap = snap;
    this.net = net;
    this.gate = new PlayheadMask((i) => snap.conversations[i].started_ms, snap.conversations.length, passMask(snap, st.filters));
    this.cursor = st.time.cursorMs;
    this.gate.seek(this.cursor);
    this.day = dayOf(snap, this.cursor);
    this.seatVisible = new Int32Array(net.unknown + 1);

    this.graph.clear();
    const { seats } = snap.meta;
    // seats on a circle unless they had a settled position before
    seats.forEach((seat, i) => {
      const prev = previous.get(seatKey(i));
      const a = (i / Math.max(1, seats.length)) * Math.PI * 2;
      this.graph.addNode(seatKey(i), {
        x: prev?.[0] ?? 100 * Math.cos(a),
        y: prev?.[1] ?? 100 * Math.sin(a),
        size: 4,
        label: seat.id,
        seat: i,
        ci: -1,
      });
    });
    if (net.links.some((l) => l.from === net.unknown || l.to === net.unknown)) {
      const prev = previous.get(seatKey(net.unknown));
      this.graph.addNode(seatKey(net.unknown), {
        x: prev?.[0] ?? 0,
        y: prev?.[1] ?? 0,
        size: 6,
        label: "unknown seat",
        seat: net.unknown,
        ci: -1,
      });
    }
    net.links.forEach((l, i) => {
      this.graph.addDirectedEdgeWithKey(`l:${i}`, seatKey(l.from), seatKey(l.to), { weight: Math.log2(1 + l.cum[l.cum.length - 1]), link: i, ci: -1 });
    });
    if (this.options.trees) this.addTrees(snap, previous);

    const harnesses = snap.meta.harnesses;
    const counts = new Array<number>(harnesses.length).fill(0);
    let unknownHarness = 0;
    for (const seat of seats) {
      if (seat.harness !== null) counts[seat.harness]++;
      else unknownHarness++;
    }
    const legend: LegendEntry[] = harnesses
      .map((h, k) => ({ key: h, label: h, color: harnessColor(h), count: counts[k] }))
      .sort((a, b) => b.count - a.count);
    if (unknownHarness) legend.push({ key: null, label: "unknown harness", color: harnessColor(null), count: unknownHarness });
    this.cb.legend(legend);
    this.refresh();
    this.startLayout();
  }

  /** Subagent trees: parent session → child subagent, with the parent hung off its seat. */
  private addTrees(snap: LoadedSnapshot, previous: Map<string, [number, number]>): void {
    const { conversations } = snap;
    const place = (ci: number, near: string | null) => {
      const key = convKey(ci);
      if (this.graph.hasNode(key)) return key;
      const prev = previous.get(key);
      const anchor = near && this.graph.hasNode(near) ? this.graph.getNodeAttributes(near) : { x: 0, y: 0 };
      const jitter = ((ci * 2654435761) % 1000) / 1000;
      this.graph.addNode(key, {
        x: prev?.[0] ?? anchor.x + 10 * Math.cos(jitter * Math.PI * 2),
        y: prev?.[1] ?? anchor.y + 10 * Math.sin(jitter * Math.PI * 2),
        size: 2,
        label: conversations[ci].label,
        seat: -1,
        ci,
      });
      return key;
    };
    snap.edges.subagent.forEach(([parent, child], i) => {
      const seat = conversations[parent].seat;
      const seatNode = seat !== null ? seatKey(seat) : null;
      const p = place(parent, seatNode);
      const c = place(child, p);
      if (!this.graph.hasEdge(p, c)) this.graph.addDirectedEdgeWithKey(`t:${i}`, p, c, { weight: 1, link: -1, ci: child });
      if (seatNode && !this.graph.hasEdge(seatNode, p)) {
        this.graph.addDirectedEdgeWithKey(`o:${parent}`, seatNode, p, { weight: 1, link: -1, ci: parent });
      }
    });
  }

  private startLayout(): void {
    this.stopLayoutFn?.();
    this.stopLayoutFn = runForceLayout(this.graph, LAYOUT_MS, [this.container, this.overlay], () => {
      this.stopLayoutFn = null;
    });
  }

  // ------------------------------------------------------------- playback

  private seek(cursor: Ms): void {
    if (!this.snap || !this.gate) return;
    this.cursor = cursor;
    const changed = this.gate.seek(cursor);
    const day = dayOf(this.snap, cursor);
    if (changed || day !== this.day) {
      this.day = day;
      this.refresh();
    } else {
      // only the pulses move within a day
      this.paintPulses();
    }
  }

  private refresh(): void {
    const { snap, gate } = this;
    if (snap && gate) {
      this.seatVisible.fill(0);
      snap.conversations.forEach((c, i) => {
        if (c.seat !== null && gate.mask[i]) this.seatVisible[c.seat]++;
      });
    }
    this.sigma.scheduleRefresh();
    this.emitStats();
  }

  private applyTheme(): void {
    this.container.style.background = css(chrome("bg"));
    this.sigma.setSetting("defaultEdgeColor", css(neutral(0.25)));
    this.refresh();
  }

  // ------------------------------------------------------------- reducers

  private reduceNode(_key: string, data: NodeAttrs): Partial<NodeDisplayData> {
    const { snap, net, gate } = this;
    if (!snap || !net || !gate) return { ...data, hidden: true };
    if (data.seat >= 0) {
      if (net.firstDay[data.seat] > this.day) return { ...data, hidden: true };
      if (data.seat === net.unknown) return { ...data, color: css(neutral(0.55)), forceLabel: true, zIndex: 2 };
      const seat = snap.meta.seats[data.seat];
      const visible = this.seatVisible[data.seat];
      const filtered = atlas.getState().filters.seats;
      const out: Partial<NodeDisplayData> = { ...data, size: 4 + 3 * Math.sqrt(visible), zIndex: 2 };
      out.color = css(visible ? harnessColor(seat.harness !== null ? snap.meta.harnesses[seat.harness] : null) : dimmed());
      if (filtered.includes(seat.id)) {
        out.color = css(accent());
        out.forceLabel = true;
        out.highlighted = true;
      }
      return out;
    }
    if (!gate.exists[data.ci]) return { ...data, hidden: true };
    const c = snap.conversations[data.ci];
    const selected = atlas.getState().selected === c.guid;
    // neutral greys: the categorical hues here already mean harness
    const tone = c.kind === "session" ? neutral(0.7) : neutral(0.48);
    return {
      ...data,
      size: c.kind === "session" ? 3 : 2,
      color: css(selected ? accent() : gate.pass[data.ci] ? tone : dimmed()),
      label: selected ? data.label : null,
      highlighted: selected,
      zIndex: 1,
    };
  }

  private reduceEdge(data: EdgeAttrs): Partial<EdgeDisplayData> {
    const { net, gate } = this;
    if (!net || !gate) return { hidden: true };
    if (data.link < 0) {
      if (!gate.exists[data.ci]) return { hidden: true };
      return { size: 0.6, color: css(neutral(0.2)), type: "line", zIndex: 0 };
    }
    const link = net.links[data.link];
    const n = messagesThrough(link, this.day);
    if (n === 0) return { hidden: true };
    const firing = messagesOn(link, this.day) > 0;
    return { size: 0.6 + 0.7 * Math.log2(1 + n), color: css(neutral(firing ? 0.7 : 0.3)), zIndex: firing ? 2 : 1 };
  }

  // ---------------------------------------------------------------- pulses

  /** One pulse per link firing today, travelling from sender to receiver as the day passes. */
  private paintPulses(): void {
    const { ctx, net, snap } = this;
    const dpr = window.devicePixelRatio || 1;
    const { width, height } = this.container.getBoundingClientRect();
    if (this.overlay.width !== Math.round(width * dpr) || this.overlay.height !== Math.round(height * dpr)) {
      this.overlay.width = Math.round(width * dpr);
      this.overlay.height = Math.round(height * dpr);
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);
    if (!net || !snap) return;
    const f = (this.cursor - snap.meta.day0_ms) / DAY_MS - this.day;
    const color = accent();
    for (const link of net.links) {
      const n = messagesOn(link, this.day);
      if (!n) continue;
      const a = this.graph.getNodeAttributes(seatKey(link.from));
      const b = this.graph.getNodeAttributes(seatKey(link.to));
      const pa = this.sigma.graphToViewport(a);
      const pb = this.sigma.graphToViewport(b);
      const x = pa.x + (pb.x - pa.x) * f;
      const y = pa.y + (pb.y - pa.y) * f;
      const r = 2.5 + Math.log2(1 + n);
      ctx.fillStyle = css(withAlpha(color, 0.25));
      ctx.beginPath();
      ctx.arc(x, y, r * 2.2, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = css(color);
      ctx.beginPath();
      ctx.arc(x, y, r, 0, Math.PI * 2);
      ctx.fill();
    }
  }

  private emitStats(): void {
    const { net, snap } = this;
    if (!net || !snap) return;
    let seats = 0;
    for (let i = 0; i < net.unknown; i++) if (net.firstDay[i] <= this.day) seats++;
    let links = 0;
    let messages = 0;
    let known = 0;
    let firing = 0;
    for (const l of net.links) {
      const n = messagesThrough(l, this.day);
      if (!n) continue;
      links++;
      messages += n;
      if (l.from !== net.unknown && l.to !== net.unknown) known += n;
      if (messagesOn(l, this.day)) firing++;
    }
    const stats: SeatsStats = {
      seats,
      links,
      messages,
      known,
      firing,
      unattributed: messagesThrough(net.unattributed, this.day),
      selfSent: messagesThrough(net.selfSent, this.day),
    };
    const key = JSON.stringify(stats);
    if (key === this.lastStats) return;
    this.lastStats = key;
    this.cb.stats(stats);
  }
}
