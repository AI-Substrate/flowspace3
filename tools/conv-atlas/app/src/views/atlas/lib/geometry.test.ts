import { describe, expect, it } from "vitest";
import { makeProjector, placeLabels } from "./geometry";

describe("makeProjector", () => {
  const identity = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];

  it("maps point space to CSS pixels like regl-scatterplot (aspect-corrected x, y down)", () => {
    const p = makeProjector(identity, 800, 400);
    expect([p.x(0, 0), p.y(0, 0)]).toEqual([400, 200]);
    // y = 1 is the top edge; x is squeezed by the 2:1 aspect so x = 1 lands at 3/4 width
    expect([p.x(1, 1), p.y(1, 1)]).toEqual([600, 0]);
    expect(p.scale).toBe(200);
  });

  it("follows camera pan and zoom", () => {
    // zoom 2 around origin, then pan +0.5 in clip x
    const view = [2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 1, 0, 0.5, 0, 0, 1];
    const p = makeProjector(view, 400, 400);
    expect(p.x(0, 0)).toBe(300);
    expect(p.x(0.25, 0)).toBe(400);
    expect(p.scale).toBe(400);
  });
});

describe("placeLabels", () => {
  const box = (x: number, y: number, priority: number) => ({ x, y, width: 40, height: 10, priority });

  it("keeps the higher-priority label when two collide", () => {
    expect(placeLabels([box(100, 100, 1), box(110, 102, 5)], 500, 500)).toEqual([1]);
  });

  it("places labels that do not overlap, highest priority first", () => {
    expect(placeLabels([box(100, 100, 1), box(300, 300, 2)], 500, 500)).toEqual([1, 0]);
  });

  it("drops labels entirely outside the viewport", () => {
    expect(placeLabels([box(-200, 100, 9), box(100, 100, 1)], 500, 500)).toEqual([1]);
  });
});
