/**
 * Overlay geometry: point space → CSS pixels with regl-scatterplot's camera,
 * and greedy collision-free label placement.
 */

export interface Projector {
  x(px: number, py: number): number;
  y(px: number, py: number): number;
  /** CSS pixels per point-space unit (uniform zoom) */
  scale: number;
}

/**
 * Mirrors regl-scatterplot's getScreenPosition (data aspect ratio 1):
 * clip = scale(1/aspect, 1) · view · [x, y, 0, 1].
 * `view` is the camera's column-major 4×4 view matrix from the "view" event.
 */
export function makeProjector(view: ArrayLike<number>, width: number, height: number): Projector {
  const aspect = width / height;
  const ax = view[0] / aspect;
  const bx = view[4] / aspect;
  const cx = view[12] / aspect;
  const ay = view[1];
  const by = view[5];
  const cy = view[13];
  return {
    x: (px, py) => (width * (ax * px + bx * py + cx + 1)) / 2,
    y: (px, py) => height * (0.5 - (ay * px + by * py + cy) / 2),
    scale: (width * Math.hypot(ax, ay)) / 2,
  };
}

export interface LabelCandidate {
  /** centre, CSS px */
  x: number;
  y: number;
  width: number;
  height: number;
  /** higher wins a collision */
  priority: number;
}

/**
 * Greedy placement: highest priority first, skip any label whose padded box
 * overlaps one already placed or leaves the viewport. Returns candidate indices.
 */
export function placeLabels(cands: readonly LabelCandidate[], viewW: number, viewH: number, pad = 4): number[] {
  const order = cands.map((_, i) => i).sort((a, b) => cands[b].priority - cands[a].priority);
  const placed: number[] = [];
  const boxes: [number, number, number, number][] = [];
  for (const i of order) {
    const c = cands[i];
    const x0 = c.x - c.width / 2 - pad;
    const y0 = c.y - c.height / 2 - pad;
    const x1 = c.x + c.width / 2 + pad;
    const y1 = c.y + c.height / 2 + pad;
    if (x1 < 0 || y1 < 0 || x0 > viewW || y0 > viewH) continue;
    if (boxes.some(([a0, b0, a1, b1]) => x0 < a1 && x1 > a0 && y0 < b1 && y1 > b0)) continue;
    boxes.push([x0, y0, x1, y1]);
    placed.push(i);
  }
  return placed;
}
