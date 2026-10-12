/**
 * Galaxy point model: which conversations are points, where they sit in the
 * scatterplot's [-1, 1] space, and how colour / size / presence are encoded
 * into regl-scatterplot's two data channels.
 *
 * Channel z (categorical, snaps): colour category; filtered-out marks use the
 * `ghost` category (palette `dimmed()`).
 * Channel w (continuous, tweens on the GPU): presence × size. w = 0 is not yet
 * existing; (0, FADE] fades in at minimum size (ghosts sit at FADE);
 * (FADE, 1] grows with sqrt(turns). Tying presence to w lets regl-scatterplot's
 * own transition animate fades and growth, so the CPU only uploads when the
 * playhead day, the filters or the colouring change.
 */
import type { LoadedSnapshot } from "../../../core/data";
import { turnsUpTo } from "../../../core/semantics";
import type { ColorCoding } from "./coloring";

export const FADE = 0.1;
export const ENCODING_STEPS = 256;
/** The home view frames this quantile range of the points, so outliers do not shrink the bulk. */
const HOME_QUANTILES: readonly [number, number] = [0.01, 0.99];
/** Margin around the home view, as a fraction of its span on each side. */
const HOME_PADDING = 0.06;

/** An area in point space, shaped like regl-scatterplot's `zoomToArea` rect. */
export interface Area {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface GalaxyLayout {
  snapshotId: string;
  /** point index → conversation index */
  convOf: Uint32Array;
  /** conversation index → point index, -1 when the conversation has no xy */
  pointOf: Int32Array;
  x: Float32Array;
  y: Float32Array;
  /** maps contract xy into point space */
  toPoint(xy: readonly [number, number]): [number, number];
  /** sqrt of the largest conversation, for size normalisation */
  maxSqrtTurns: number;
  /** the camera's home view: the robust extent of the points plus padding */
  home: Area;
}

export function buildLayout(s: LoadedSnapshot): GalaxyLayout {
  const [x0, y0, x1, y1] = s.meta.layout.bounds;
  const span = Math.max(x1 - x0, y1 - y0) || 1;
  const k = 1.9 / span;
  const cx = (x0 + x1) / 2;
  const cy = (y0 + y1) / 2;
  const toPoint = (xy: readonly [number, number]): [number, number] => [(xy[0] - cx) * k, (xy[1] - cy) * k];

  const pointOf = new Int32Array(s.conversations.length).fill(-1);
  const conv: number[] = [];
  let maxTurns = 1;
  s.conversations.forEach((c, i) => {
    if (c.xy) {
      pointOf[i] = conv.length;
      conv.push(i);
    }
    if (c.turns > maxTurns) maxTurns = c.turns;
  });
  const convOf = Uint32Array.from(conv);
  const x = new Float32Array(convOf.length);
  const y = new Float32Array(convOf.length);
  convOf.forEach((ci, p) => {
    [x[p], y[p]] = toPoint(s.conversations[ci].xy as [number, number]);
  });
  return { snapshotId: s.id, convOf, pointOf, x, y, toPoint, maxSqrtTurns: Math.sqrt(maxTurns), home: homeArea(x, y) };
}

/**
 * The p1–p99 box of the points, padded. Layout bounds describe the projection,
 * not where the points actually landed, so fitting to them can leave whole
 * margins of the stage empty.
 */
export function homeArea(x: Float32Array, y: Float32Array): Area {
  if (x.length === 0) return { x: -1, y: -1, width: 2, height: 2 };
  const [lo, hi] = HOME_QUANTILES;
  const xs = Float32Array.from(x).sort();
  const ys = Float32Array.from(y).sort();
  const at = (sorted: Float32Array, q: number) => sorted[Math.round(q * (sorted.length - 1))];
  const [x0, x1, y0, y1] = [at(xs, lo), at(xs, hi), at(ys, lo), at(ys, hi)];
  // a single point or a line still needs a non-degenerate frame
  const span = Math.max(x1 - x0, y1 - y0, 0.05);
  const w = Math.max(x1 - x0, span * 0.05) * (1 + 2 * HOME_PADDING);
  const h = Math.max(y1 - y0, span * 0.05) * (1 + 2 * HOME_PADDING);
  return { x: (x0 + x1 - w) / 2, y: (y0 + y1 - h) / 2, width: w, height: h };
}

/**
 * Fills `out` (per point) with the w channel: 0 = not yet existing, FADE = a
 * filtered-out ghost, above FADE = visible, sized by turns up to `day`.
 */
export function encodePresence(
  s: LoadedSnapshot,
  layout: GalaxyLayout,
  exists: Uint8Array,
  pass: Uint8Array,
  day: number,
  out: Float32Array,
): Float32Array {
  const { convOf, maxSqrtTurns } = layout;
  for (let p = 0; p < convOf.length; p++) {
    const ci = convOf[p];
    if (!exists[ci]) out[p] = 0;
    else if (!pass[ci]) out[p] = FADE;
    else out[p] = FADE + (1 - FADE) * Math.min(1, Math.sqrt(turnsUpTo(s, ci, day)) / maxSqrtTurns);
  }
  return out;
}

export interface PresenceEncoding {
  sizes: number[];
  opacities: number[];
}

/**
 * Point sizes (px) and opacities indexed by w, as regl-scatterplot encoding arrays.
 * Fading is opacity only: sizes never drop below minSize, because regl-scatterplot
 * derives its zoom-out floor from sizes[0] (MIN_POINT_SIZE / sizes[0]), and a 0
 * there makes every point render at the GPU's maximum point size.
 */
export function presenceEncoding(minSize: number, maxSize: number, maxOpacity: number): PresenceEncoding {
  const sizes: number[] = [];
  const opacities: number[] = [];
  for (let k = 0; k < ENCODING_STEPS; k++) {
    const w = k / (ENCODING_STEPS - 1);
    const grow = w <= FADE ? 0 : (w - FADE) / (1 - FADE);
    sizes.push(minSize + (maxSize - minSize) * grow);
    opacities.push(Math.min(1, w / FADE) * maxOpacity);
  }
  return { sizes, opacities };
}

/** Fills `out` (per point) with the z channel: the point's category, or the ghost when filtered out. */
export function encodeColor(coding: ColorCoding, layout: GalaxyLayout, pass: Uint8Array, out: Float32Array): Float32Array {
  for (let p = 0; p < out.length; p++) {
    const ci = layout.convOf[p];
    out[p] = pass[ci] ? coding.category[ci] : coding.ghost;
  }
  return out;
}
