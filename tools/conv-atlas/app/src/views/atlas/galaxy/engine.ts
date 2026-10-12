/**
 * Imperative galaxy renderer. React mounts it once; from then on it talks to
 * the store directly (atlas.subscribe) so playback never re-renders React.
 *
 * GPU layer: regl-scatterplot, with colour in z and presence × size in w (see
 * lib/galaxyModel). Draws are serialised and coalesced: regl-scatterplot rejects
 * overlapping draw() calls, and we only need the latest state anyway.
 * Overlay layer: one 2D canvas for edges, cluster labels, live pulses, the
 * hover ring and insight halos, repainted on camera/playhead/state changes and
 * animated only while playing or while an insight is highlighted.
 */
import createScatterplot from "regl-scatterplot";
import type { InsightKind, Ms } from "../../../contract/types";
import type { LoadedSnapshot } from "../../../core/data";
import { dayOf, liveAt } from "../../../core/semantics";
import { type AtlasState, atlas } from "../../../core/store";
import { accent, chrome, css, kindColor, neutral, type RGBA, withAlpha } from "../../../design/palette";
import { getTheme, onThemeChange } from "../../../design/theme";
import { type ColorCoding, colorCoding, type LegendEntry, recountLegend } from "../lib/coloring";
import { buildLayout, encodeColor, encodePresence, type GalaxyLayout, presenceEncoding } from "../lib/galaxyModel";
import { countAtOrBefore, PlayheadMask, passMask, type TimeOrder, timeOrder } from "../lib/gate";
import { makeProjector, placeLabels, type Projector } from "../lib/geometry";
import { matchTween, tweenPresence } from "../lib/tween";

/** regl-scatterplot does not export a name for its instance type. */
type Scatterplot = ReturnType<typeof createScatterplot>;

export type EdgeMode = "off" | "all" | "cross";

export interface GalaxyOptions {
  edges: EdgeMode;
  labels: boolean;
}

/** Hovered conversation and its position in viewport (client) pixels. */
export interface TooltipAnchor {
  guid: string;
  x: number;
  y: number;
}

export interface GalaxyStats {
  /** visible at the playhead and passing filters */
  visible: number;
  /** started by the playhead (visible + filtered-out ghosts) */
  existing: number;
  /** points in the map (conversations with a layout position) */
  points: number;
}

export interface GalaxyCallbacks {
  tooltip(t: TooltipAnchor | null): void;
  legend(entries: LegendEntry[]): void;
  stats(s: GalaxyStats): void;
}

const MIN_SIZE = 3;
const MAX_SIZE = 18;
const PRESENCE_MS = 240;
const SWAP_MS = 1400;
const ZOOM_MS = 600;
const MAX_LIVE_RINGS = 600;
const ENCODING = presenceEncoding(MIN_SIZE, MAX_SIZE, 0.92);
const DRAW_TYPES = { zDataType: "categorical", wDataType: "continuous" } as const;
const REDUCED_MOTION = "(prefers-reduced-motion: reduce)";

/** Everything derived from one snapshot. */
interface Scene {
  snap: LoadedSnapshot;
  layout: GalaxyLayout;
  gate: PlayheadMask;
  color: ColorCoding;
  z: Float32Array;
  w: Float32Array;
  day: number;
  knn: TimeOrder;
  /** visible points per community, refreshed with the mask */
  communityVisible: Int32Array;
  spatialIndex?: ArrayBuffer;
}

export class GalaxyEngine {
  private readonly plot: Scatterplot;
  private readonly ctx: CanvasRenderingContext2D;
  private scene: Scene | null = null;
  private options: GalaxyOptions;
  private view: ArrayLike<number> = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];
  private baseScale = 0;
  private width = 1;
  private height = 1;
  private busy = false;
  /**
   * Set by destroy(). Async work checks it after every await: regl-scatterplot
   * resolves an in-flight draw() silently when destroyed mid-draw, and any call
   * after that (e.g. zoomToArea) hits its disposed camera.
   */
  private destroyed = false;
  /** openSnapshot calls waiting for the renderer; served before queued presence draws */
  private readonly idleWaiters: (() => void)[] = [];
  private presenceDirty = false;
  private overlayDirty = true;
  private raf = 0;
  private lassoing = false;
  /** point under the tooltip, -1 when hidden */
  private tipPoint = -1;
  /** last header stats sent to React; re-sent only when they change */
  private lastStats: GalaxyStats | null = null;
  /** visible points per colour category, recounted with the mask */
  private legendCounts = new Int32Array(0);
  /** the colouring and counts behind the last legend sent to React */
  private lastLegend: { color: ColorCoding | null; counts: string } = { color: null, counts: "" };
  private halo: { guids: string[]; kind: InsightKind } = { guids: [], kind: "finding" };
  /** canvas text uses the UI family from tokens.css */
  private readonly fontFamily = getComputedStyle(document.documentElement).getPropertyValue("--font-sans").trim() || "sans-serif";
  private readonly unsubs: (() => void)[] = [];
  private readonly resize: ResizeObserver;

  constructor(
    private readonly host: HTMLElement,
    private readonly glCanvas: HTMLCanvasElement,
    private readonly overlay: HTMLCanvasElement,
    private readonly cb: GalaxyCallbacks,
    options: GalaxyOptions,
  ) {
    this.options = options;
    const ctx = overlay.getContext("2d");
    if (!ctx) throw new Error("galaxy: 2D canvas unavailable");
    this.ctx = ctx;
    const rect = host.getBoundingClientRect();
    this.width = Math.max(1, rect.width);
    this.height = Math.max(1, rect.height);
    this.plot = createScatterplot({
      canvas: glCanvas,
      width: this.width,
      height: this.height,
      colorBy: "valueZ",
      sizeBy: "valueW",
      opacityBy: "valueW",
      pointSize: ENCODING.sizes,
      opacity: ENCODING.opacities,
      pointOutlineWidth: 2,
      opacityInactiveMax: 0.55,
      deselectOnDblClick: false,
      deselectOnEscape: true,
      showReticle: false,
    });
    this.applyChrome();
    this.resizeOverlay();
    this.resize = new ResizeObserver(() => this.onResize());
    this.resize.observe(host);
    this.wirePlot();
    this.wireStore();
    this.unsubs.push(onThemeChange(() => this.onTheme()));
    glCanvas.addEventListener("dblclick", this.onDblClick);
    this.requestFrame();
  }

  destroy(): void {
    this.destroyed = true;
    cancelAnimationFrame(this.raf);
    for (const u of this.unsubs) u();
    this.glCanvas.removeEventListener("dblclick", this.onDblClick);
    this.resize.disconnect();
    this.plot.destroy();
  }

  setOptions(options: GalaxyOptions): void {
    this.options = options;
    this.markOverlay();
  }

  /** Keyboard counterpart of double-click: zoom to the selection's cluster, or home without one. */
  zoomToSelection(): void {
    const sc = this.scene;
    if (sc) this.zoomToCommunity(sc, this.communityOf(sc, atlas.getState().selected));
  }

  /** Keyboard counterpart of the lasso: picks the visible members of the selection's cluster. */
  pickSelectionCluster(): void {
    const sc = this.scene;
    const community = sc ? this.communityOf(sc, atlas.getState().selected) : null;
    if (!sc || community === null) return;
    const guids: string[] = [];
    sc.snap.conversations.forEach((c, i) => {
      if (c.community === community && sc.layout.pointOf[i] >= 0 && sc.gate.mask[i]) guids.push(c.guid);
    });
    atlas.getState().pick(guids);
  }

  // ---------------------------------------------------------------- wiring

  private wirePlot(): void {
    const p = this.plot;
    p.subscribe("view", ({ view }) => {
      this.view = view;
      if (this.tipPoint >= 0) this.showTooltip(this.tipPoint);
      this.markOverlay();
    });
    p.subscribe("pointOver", (pt) => {
      const guid = this.guidAtPoint(pt);
      if (!guid) return;
      atlas.getState().hover(guid);
      this.tipPoint = pt;
      this.showTooltip(pt);
    });
    p.subscribe("pointOut", () => {
      atlas.getState().hover(null);
      this.tipPoint = -1;
      this.cb.tooltip(null);
    });
    p.subscribe("lassoStart", () => {
      this.lassoing = true;
    });
    p.subscribe("lassoEnd", () => {
      this.lassoing = false;
    });
    p.subscribe("select", ({ points }) => {
      const s = atlas.getState();
      const guids = points.map((pt) => this.guidAtPoint(pt)).filter((g): g is string => g !== null);
      if (this.lassoing || guids.length > 1) {
        s.pick(guids);
      } else {
        s.pick([]);
        s.select(guids[0] ?? null);
      }
      // the GPU picker also hits hidden points; the store is the truth
      this.syncSelection();
    });
    p.subscribe("deselect", () => {
      const s = atlas.getState();
      s.select(null);
      s.pick([]);
    });
  }

  /** Anchors the tooltip to point `pt` in viewport pixels; camera moves call this again. */
  private showTooltip(pt: number): void {
    const guid = this.guidAtPoint(pt);
    const pos = guid ? this.plot.getScreenPosition(pt) : undefined;
    if (!guid || !pos) {
      this.tipPoint = -1;
      this.cb.tooltip(null);
      return;
    }
    const rect = this.host.getBoundingClientRect();
    this.cb.tooltip({ guid, x: rect.left + pos[0], y: rect.top + pos[1] });
  }

  private wireStore(): void {
    const sub = <T>(sel: (s: AtlasState) => T, fn: (v: T) => void) => this.unsubs.push(atlas.subscribe(sel, fn));
    sub(
      (s) => s.snapshot,
      (snap) => {
        if (snap) void this.openSnapshot(snap);
      },
    );
    sub(
      (s) => s.filters,
      () => {
        const sc = this.scene;
        if (!sc) return;
        sc.gate.setPass(passMask(sc.snap, atlas.getState().filters));
        this.presenceChanged();
      },
    );
    sub(
      (s) => s.time.cursorMs,
      (cursor) => this.seek(cursor),
    );
    sub(
      (s) => s.time.playing,
      () => this.markOverlay(),
    );
    sub(
      (s) => s.colorBy,
      () => this.recolor(),
    );
    sub(
      (s) => s.selected,
      () => this.syncSelection(),
    );
    sub(
      (s) => s.picked,
      () => this.syncSelection(),
    );
    sub(
      (s) => s.hovered,
      () => this.markOverlay(),
    );
    sub(
      (s) => s.insight,
      (insight) => {
        const guids = new Set(insight?.refs.guids ?? []);
        for (const a of insight?.refs.addresses ?? []) {
          const m = /^conv:([^#]+)/.exec(a);
          if (m) guids.add(m[1]);
        }
        this.halo = { guids: [...guids], kind: insight?.kind ?? "finding" };
        this.markOverlay();
      },
    );
    const snap = atlas.getState().snapshot;
    if (snap) void this.openSnapshot(snap);
  }

  // ---------------------------------------------------------------- colour

  /** Background and lasso colours for the current theme. */
  private applyChrome(): void {
    this.plot.set({ backgroundColor: chrome("bg"), lassoColor: accent() });
  }

  /**
   * Category colours plus selection (accent) and hover (ink) states. regl-scatterplot
   * only honours active/hover colours given per category; a single colour falls back
   * to the point's own.
   */
  private applyColors(colors: ColorCoding["colors"]): void {
    // the .d.ts types these as one colour, but setColors/getColors accept and need one per category
    const active = colors.map(() => accent()) as unknown as RGBA;
    const hover = colors.map(() => chrome("ink")) as unknown as RGBA;
    this.plot.set({ pointColor: colors, pointColorActive: active, pointColorHover: hover });
  }

  private onTheme(): void {
    this.applyChrome();
    this.recolor();
  }

  private recolor(): void {
    const sc = this.scene;
    if (!sc) return;
    sc.color = colorCoding(sc.snap, atlas.getState().colorBy, getTheme());
    this.applyColors(sc.color.colors);
    this.presenceChanged();
  }

  // ------------------------------------------------------------- snapshots

  private buildScene(snap: LoadedSnapshot): Scene {
    const st = atlas.getState();
    const layout = buildLayout(snap);
    const n = layout.convOf.length;
    const knnEdges = snap.edges.knn.edges;
    const sc: Scene = {
      snap,
      layout,
      gate: new PlayheadMask((i) => snap.conversations[i].started_ms, snap.conversations.length, passMask(snap, st.filters)),
      color: colorCoding(snap, st.colorBy, getTheme()),
      z: new Float32Array(n),
      w: new Float32Array(n),
      day: 0,
      knn: timeOrder(knnEdges.length, (e) => knnEdges[e][3]),
      communityVisible: new Int32Array(snap.meta.communities.length),
    };
    this.syncScene(sc);
    return sc;
  }

  /** Brings a scene up to the store: colouring, filters, playhead; re-encodes z and w. */
  private syncScene(sc: Scene): void {
    const st = atlas.getState();
    sc.color = colorCoding(sc.snap, st.colorBy, getTheme());
    sc.gate.setPass(passMask(sc.snap, st.filters));
    sc.gate.seek(st.time.cursorMs);
    sc.day = dayOf(sc.snap, st.time.cursorMs);
    this.encode(sc);
  }

  private encode(sc: Scene): void {
    encodeColor(sc.color, sc.layout, sc.gate.pass, sc.z);
    encodePresence(sc.snap, sc.layout, sc.gate.exists, sc.gate.pass, sc.day, sc.w);
    this.countVisible(sc);
  }

  private async openSnapshot(snap: LoadedSnapshot): Promise<void> {
    if (this.scene?.snap === snap) return;
    const prev = this.scene;
    const next = this.buildScene(snap);
    while (this.busy) await this.idle();
    if (this.destroyed) return;
    this.busy = true;
    try {
      this.applyColors(next.color.colors);
      if (prev && prev.layout.convOf.length > 0 && !matchMedia(REDUCED_MOTION).matches) {
        // a failed tween only loses the animation; the final draw below still installs the snapshot
        await this.swapTween(prev, next).catch((e: unknown) => {
          if (!this.destroyed) console.error("galaxy: swap tween failed", e);
        });
        if (this.destroyed) return;
        // store changes that landed during the tween were not applied to `next`
        this.syncScene(next);
        this.applyColors(next.color.colors);
      }
      this.scene = next;
      // positions are final now; let regl build the spatial index once, then reuse it
      await this.plot.draw({ x: next.layout.x, y: next.layout.y, z: next.z, w: next.w }, { ...DRAW_TYPES });
      if (this.destroyed) return;
      next.spatialIndex = this.plot.get("spatialIndex");
      this.presenceDirty = false;
      // frame the new snapshot; the first one snaps into place rather than flying in
      const transition = prev !== null && !matchMedia(REDUCED_MOTION).matches;
      void this.plot.zoomToArea(next.layout.home, { transition, transitionDuration: ZOOM_MS });
    } finally {
      // a destroyed engine never draws again: leave it busy so queued work stays parked
      if (!this.destroyed) {
        this.markOverlay();
        this.release();
      }
    }
  }

  /** Tween the outgoing points to the incoming layout, matched by guid. */
  private async swapTween(prev: Scene, next: Scene): Promise<void> {
    const u = matchTween(
      prev.layout,
      prev.snap.conversations.map((c) => c.guid),
      next.layout,
      next.snap.conversations.map((c) => c.guid),
    );
    const { from, to } = tweenPresence(u, prev.w, next.w);
    const z = new Float32Array(u.status.length).fill(next.color.ghost);
    z.set(next.z);
    this.scene = null; // the overlay pauses during the tween: point indices are in flux
    this.markOverlay();
    this.tipPoint = -1;
    this.cb.tooltip(null);
    await this.plot.draw({ x: u.fromX, y: u.fromY, z, w: from }, { ...DRAW_TYPES });
    await this.plot.draw({ x: u.toX, y: u.toY, z, w: to }, { ...DRAW_TYPES, transition: true, transitionDuration: SWAP_MS });
  }

  // ------------------------------------------------------------- playback

  private seek(cursor: Ms): void {
    const sc = this.scene;
    if (!sc) return;
    const changed = sc.gate.seek(cursor);
    const day = dayOf(sc.snap, cursor);
    if (changed || day !== sc.day) {
      sc.day = day;
      this.presenceChanged();
    } else {
      this.markOverlay();
    }
  }

  private presenceChanged(): void {
    const sc = this.scene;
    if (!sc) return;
    this.countVisible(sc);
    this.presenceDirty = true;
    this.markOverlay();
    this.flushPresence();
  }

  /** Serialised, coalesced upload of z and w; the GPU tweens w between uploads. */
  private flushPresence(): void {
    const sc = this.scene;
    if (this.destroyed || this.busy || !this.presenceDirty || !sc) return;
    this.presenceDirty = false;
    this.busy = true;
    this.encode(sc);
    const animate = !matchMedia(REDUCED_MOTION).matches;
    this.plot
      .draw(
        { x: sc.layout.x, y: sc.layout.y, z: sc.z, w: sc.w },
        {
          ...DRAW_TYPES,
          transition: animate,
          transitionDuration: PRESENCE_MS,
          preventFilterReset: true,
          spatialIndex: sc.spatialIndex,
        },
      )
      .catch((e: unknown) => console.error("galaxy: presence draw failed", e))
      .finally(() => this.release());
  }

  /** Ends a draw: hands the renderer to a waiting swap first, else flushes queued presence. */
  private release(): void {
    this.busy = false;
    const waiter = this.idleWaiters.shift();
    if (waiter) {
      waiter();
      return;
    }
    this.syncSelection();
    this.flushPresence();
  }

  private idle(): Promise<void> {
    // executor form: the app's tsconfig targets lib ES2023, which lacks Promise.withResolvers
    return new Promise((resolve) => this.idleWaiters.push(resolve));
  }

  /** Visible points per community (drives cluster labels) and per colour category (the legend), and the header stats. */
  private countVisible(sc: Scene): void {
    sc.communityVisible.fill(0);
    if (this.legendCounts.length !== sc.color.colors.length) this.legendCounts = new Int32Array(sc.color.colors.length);
    else this.legendCounts.fill(0);
    const { conversations } = sc.snap;
    const { mask, exists } = sc.gate;
    let visible = 0;
    let existing = 0;
    for (let i = 0; i < conversations.length; i++) {
      if (sc.layout.pointOf[i] < 0) continue;
      existing += exists[i];
      if (!mask[i]) continue;
      visible++;
      this.legendCounts[sc.color.category[i]]++;
      const c = conversations[i].community;
      if (c !== null) sc.communityVisible[c]++;
    }
    const counts = this.legendCounts.join();
    if (this.lastLegend.color !== sc.color || this.lastLegend.counts !== counts) {
      this.lastLegend = { color: sc.color, counts };
      this.cb.legend(recountLegend(sc.color, this.legendCounts));
    }
    const last = this.lastStats;
    const points = sc.layout.convOf.length;
    if (last && last.visible === visible && last.existing === existing && last.points === points) return;
    this.lastStats = { visible, existing, points };
    this.cb.stats(this.lastStats);
  }

  // ------------------------------------------------------------ selection

  private guidAtPoint(pt: number): string | null {
    const sc = this.scene;
    if (!sc || pt < 0 || pt >= sc.layout.convOf.length) return null;
    const ci = sc.layout.convOf[pt];
    return sc.gate.mask[ci] ? sc.snap.conversations[ci].guid : null;
  }

  private pointOfGuid(guid: string): number {
    const sc = this.scene;
    const ci = sc?.snap.guidIndex.get(guid);
    return sc && ci !== undefined ? sc.layout.pointOf[ci] : -1;
  }

  private syncSelection(): void {
    if (!this.scene || this.busy) return;
    const { selected, picked } = atlas.getState();
    const guids = picked.length ? picked : selected ? [selected] : [];
    const pts = guids.map((g) => this.pointOfGuid(g)).filter((p) => p >= 0);
    if (pts.length) this.plot.select(pts, { preventEvent: true });
    else this.plot.deselect({ preventEvent: true });
    this.markOverlay();
  }

  /** Double-click a point: zoom to its community's visible members; empty space: back to the home view. */
  private readonly onDblClick = (): void => {
    const sc = this.scene;
    if (sc) this.zoomToCommunity(sc, this.communityOf(sc, atlas.getState().hovered));
  };

  private communityOf(sc: Scene, guid: string | null): number | null {
    const ci = guid ? sc.snap.guidIndex.get(guid) : undefined;
    return ci === undefined ? null : sc.snap.conversations[ci].community;
  }

  /** Frames a community's visible members; no community, or none visible, frames the home view. */
  private zoomToCommunity(sc: Scene, community: number | null): void {
    const transition = !matchMedia(REDUCED_MOTION).matches;
    const members: number[] = [];
    if (community !== null) {
      sc.snap.conversations.forEach((c, i) => {
        const p = sc.layout.pointOf[i];
        if (c.community === community && p >= 0 && sc.gate.mask[i]) members.push(p);
      });
    }
    if (members.length) void this.plot.zoomToPoints(members, { padding: 0.25, transition, transitionDuration: ZOOM_MS });
    else void this.plot.zoomToArea(sc.layout.home, { transition, transitionDuration: ZOOM_MS });
  }

  // --------------------------------------------------------------- overlay

  private onResize(): void {
    const rect = this.host.getBoundingClientRect();
    this.width = Math.max(1, rect.width);
    this.height = Math.max(1, rect.height);
    this.plot.set({ width: this.width, height: this.height });
    this.resizeOverlay();
    this.markOverlay();
  }

  private resizeOverlay(): void {
    const dpr = window.devicePixelRatio || 1;
    this.overlay.width = Math.round(this.width * dpr);
    this.overlay.height = Math.round(this.height * dpr);
    this.overlay.style.width = `${this.width}px`;
    this.overlay.style.height = `${this.height}px`;
  }

  private markOverlay(): void {
    this.overlayDirty = true;
    this.requestFrame();
  }

  private requestFrame(): void {
    if (!this.raf) this.raf = requestAnimationFrame(this.frame);
  }

  private readonly frame = (now: number): void => {
    this.raf = 0;
    if (this.paintOverlay(now)) this.requestFrame();
  };

  /** Repaints the overlay; returns true while something is animating. */
  private paintOverlay(now: number): boolean {
    const sc = this.scene;
    const st = atlas.getState();
    const motion = !matchMedia(REDUCED_MOTION).matches;
    const animate = motion && sc !== null && (this.halo.guids.length > 0 || st.time.playing);
    if (!this.overlayDirty && !animate) return false;
    this.overlayDirty = false;
    const { ctx } = this;
    const dpr = window.devicePixelRatio || 1;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, this.width, this.height);
    if (!sc) return false;

    const proj = makeProjector(this.view, this.width, this.height);
    if (!this.baseScale) this.baseScale = proj.scale;
    const zoom = proj.scale / this.baseScale;
    const cursor = st.time.cursorMs;
    const { conversations } = sc.snap;
    const { mask } = sc.gate;
    const { pointOf, x, y } = sc.layout;
    // CSS px radius of point p, matching the GPU size encoding and its asinh zoom scaling
    const zoomPx = zoom > 1 ? Math.asinh(zoom) / Math.asinh(1) : zoom;
    const radius = (p: number) => (ENCODING.sizes[Math.floor(sc.w[p] * (ENCODING.sizes.length - 1))] * zoomPx) / 2;
    const t = motion ? now / 1000 : 0;

    if (this.options.edges !== "off") this.paintEdges(sc, proj, cursor);

    // live at the playhead: a soft accent ring, breathing while playing
    const live = accent();
    ctx.lineWidth = 1.25;
    let rings = 0;
    for (let i = 0; i < conversations.length && rings < MAX_LIVE_RINGS; i++) {
      const p = pointOf[i];
      if (p < 0 || !mask[i] || !liveAt(conversations[i], cursor)) continue;
      rings++;
      const wave = st.time.playing ? 0.5 + 0.5 * Math.sin(t * 4 + i) : 0.5;
      ctx.strokeStyle = css(withAlpha(live, 0.2 + 0.35 * wave));
      ctx.beginPath();
      ctx.arc(proj.x(x[p], y[p]), proj.y(x[p], y[p]), radius(p) + 2 + 3 * wave, 0, Math.PI * 2);
      ctx.stroke();
    }

    // insight halos, in the insight kind's colour
    const haloColor = kindColor(this.halo.kind);
    const wave = 0.5 + 0.5 * Math.sin(t * 2.5);
    for (const g of this.halo.guids) {
      const p = this.pointOfGuid(g);
      if (p < 0) continue;
      const px = proj.x(x[p], y[p]);
      const py = proj.y(x[p], y[p]);
      ctx.strokeStyle = css(withAlpha(haloColor, 0.95));
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.arc(px, py, radius(p) + 5 + 2 * wave, 0, Math.PI * 2);
      ctx.stroke();
      ctx.strokeStyle = css(withAlpha(haloColor, 0.3 * (1 - wave)));
      ctx.lineWidth = 6;
      ctx.beginPath();
      ctx.arc(px, py, radius(p) + 10 + 6 * wave, 0, Math.PI * 2);
      ctx.stroke();
    }

    // hover ring (also mirrors hovers coming from other views)
    const hp = st.hovered ? this.pointOfGuid(st.hovered) : -1;
    if (hp >= 0 && mask[sc.layout.convOf[hp]]) {
      ctx.lineWidth = 1.5;
      ctx.strokeStyle = css(chrome("ink"));
      ctx.beginPath();
      ctx.arc(proj.x(x[hp], y[hp]), proj.y(x[hp], y[hp]), radius(hp) + 4, 0, Math.PI * 2);
      ctx.stroke();
    }

    if (this.options.labels) this.paintLabels(sc, proj, zoom);
    return animate;
  }

  /** kNN edges born by the playhead between visible points; cross-repo edges brighter and wider. */
  private paintEdges(sc: Scene, proj: Projector, cursor: Ms): void {
    const { ctx } = this;
    const edges = sc.snap.edges.knn.edges;
    const { conversations } = sc.snap;
    const { mask } = sc.gate;
    const { pointOf, x, y } = sc.layout;
    const count = countAtOrBefore(sc.knn.times, cursor);
    const same = new Path2D();
    const cross = new Path2D();
    for (let k = 0; k < count; k++) {
      const [a, b] = edges[sc.knn.order[k]];
      const pa = pointOf[a];
      const pb = pointOf[b];
      if (pa < 0 || pb < 0 || !mask[a] || !mask[b]) continue;
      const ra = conversations[a].repo;
      const rb = conversations[b].repo;
      const isCross = ra !== null && rb !== null && ra !== rb;
      if (!isCross && this.options.edges === "cross") continue;
      const path = isCross ? cross : same;
      path.moveTo(proj.x(x[pa], y[pa]), proj.y(x[pa], y[pa]));
      path.lineTo(proj.x(x[pb], y[pb]), proj.y(x[pb], y[pb]));
    }
    ctx.lineWidth = 1;
    ctx.strokeStyle = css(withAlpha(neutral(0.5), 0.22));
    ctx.stroke(same);
    ctx.lineWidth = 1.5;
    ctx.strokeStyle = css(withAlpha(neutral(0.92), 0.6));
    ctx.stroke(cross);
  }

  /** Community labels at their centroids on a backplate, biggest visible first, never overlapping. */
  private paintLabels(sc: Scene, proj: Projector, zoom: number): void {
    const { ctx } = this;
    const fontPx = Math.min(20, Math.max(11, 12 * zoom ** 0.3));
    ctx.font = `600 ${fontPx}px ${this.fontFamily}`;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    const comms = sc.snap.meta.communities;
    const idx: number[] = [];
    const cands = [];
    for (let c = 0; c < comms.length; c++) {
      const visible = sc.communityVisible[c];
      if (!visible) continue;
      const [px, py] = sc.layout.toPoint(comms[c].centroid_xy);
      idx.push(c);
      cands.push({
        x: proj.x(px, py),
        y: proj.y(px, py),
        width: ctx.measureText(comms[c].label).width,
        height: fontPx,
        priority: visible,
      });
    }
    // a translucent plate keeps the label legible over dense points of any colour
    const plate = css(withAlpha(chrome("bg"), 0.78));
    const ink = css(chrome("ink"));
    const [padX, padY] = [6, 3];
    for (const k of placeLabels(cands, this.width, this.height, 2 + padX)) {
      const c = cands[k];
      ctx.fillStyle = plate;
      ctx.beginPath();
      ctx.roundRect(c.x - c.width / 2 - padX, c.y - c.height / 2 - padY, c.width + 2 * padX, c.height + 2 * padY, 4);
      ctx.fill();
      ctx.fillStyle = ink;
      ctx.fillText(comms[idx[k]].label, c.x, c.y);
    }
  }
}
