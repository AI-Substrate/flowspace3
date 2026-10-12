/**
 * Theme-aware label and hover drawing for the sigma views. Sigma's defaults
 * paint a white hover box, which is unreadable on the dark theme; these read
 * the current palette on every draw. Node labels are placed by our own
 * collision pass (lib/geometry) rather than sigma's coarse label grid.
 */
import type Sigma from "sigma";
import type { Settings } from "sigma/settings";
import type { NodeDisplayData, PartialButFor, PlainObject } from "sigma/types";
import { chrome, css } from "../../design/palette";
import { type LabelCandidate, placeLabels } from "./lib/geometry";

type LabelData = PartialButFor<NodeDisplayData, "x" | "y" | "size" | "label" | "color">;
/** only the label settings: keeps the drawers assignable to any sigma graph typing */
type LabelSettings = Pick<Settings, "labelSize" | "labelWeight" | "labelFont">;

/**
 * Returns an `afterRender` handler that draws node labels on sigma's label
 * layer: nodes at least `minNodePx` on screen, or forced (selection, halos),
 * forced first then by size, skipping any label that would overlap a placed
 * one. The sigma instance must be created with `renderLabels: false`.
 */
export function placedLabelPainter<N extends PlainObject, E extends PlainObject>(sigma: Sigma<N, E>, minNodePx: number): () => void {
  const ctx = sigma.getCanvases().labels.getContext("2d");
  if (!ctx) throw new Error("sigma: 2D label canvas unavailable");
  const settings: LabelSettings = {
    labelSize: sigma.getSetting("labelSize"),
    labelWeight: sigma.getSetting("labelWeight"),
    labelFont: sigma.getSetting("labelFont"),
  };
  /** label text → width in px at the label font */
  const widths = new Map<string, number>();
  return () => {
    const { width, height } = sigma.getDimensions();
    const data: LabelData[] = [];
    const cands: LabelCandidate[] = [];
    ctx.font = `${settings.labelWeight} ${settings.labelSize}px ${settings.labelFont}`;
    sigma.getGraph().forEachNode((key) => {
      const d = sigma.getNodeDisplayData(key);
      if (!d || d.hidden || !d.label) return;
      const size = sigma.scaleSize(d.size);
      if (!d.forceLabel && size < minNodePx) return;
      const p = sigma.framedGraphToViewport(d);
      let w = widths.get(d.label);
      if (w === undefined) {
        w = ctx.measureText(d.label).width;
        widths.set(d.label, w);
      }
      data.push({ ...d, x: p.x, y: p.y, size });
      // the label starts just right of the node (see drawThemedNodeLabel)
      cands.push({ x: p.x + size + 3 + w / 2, y: p.y, width: w, height: settings.labelSize, priority: (d.forceLabel ? 1e6 : 0) + size });
    });
    for (const k of placeLabels(cands, width, height, 2)) drawThemedNodeLabel(ctx, data[k], settings);
  };
}

export function drawThemedNodeLabel(ctx: CanvasRenderingContext2D, data: LabelData, settings: LabelSettings): void {
  if (!data.label) return;
  const size = settings.labelSize;
  ctx.font = `${settings.labelWeight} ${size}px ${settings.labelFont}`;
  ctx.lineJoin = "round";
  ctx.lineWidth = 3;
  ctx.strokeStyle = css(chrome("bg"));
  ctx.fillStyle = css(chrome("ink"));
  const x = data.x + data.size + 3;
  const y = data.y + size / 3;
  ctx.strokeText(data.label, x, y);
  ctx.fillText(data.label, x, y);
}

export function drawThemedNodeHover(ctx: CanvasRenderingContext2D, data: LabelData, settings: LabelSettings): void {
  const size = settings.labelSize;
  ctx.font = `${settings.labelWeight} ${size}px ${settings.labelFont}`;
  const pad = 3;
  if (data.label) {
    const w = ctx.measureText(data.label).width;
    const h = size + 2 * pad;
    ctx.fillStyle = css(chrome("raised"));
    ctx.beginPath();
    ctx.roundRect(data.x + data.size + 1, data.y - h / 2, w + 2 * pad + 2, h, 4);
    ctx.fill();
    ctx.fillStyle = css(chrome("ink"));
    ctx.fillText(data.label, data.x + data.size + 2 + pad, data.y + size / 3);
  }
  ctx.strokeStyle = css(chrome("ink"));
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  ctx.arc(data.x, data.y, data.size + 3, 0, Math.PI * 2);
  ctx.stroke();
}
