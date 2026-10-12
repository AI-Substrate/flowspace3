import { beforeEach, describe, expect, it } from "vitest";
import { resetColorSlots } from "../../../core/colorSlots";
import { DAY_MS, NO_FILTERS } from "../../../core/semantics";
import { communityColor } from "../../../design/palette";
import { colorCoding } from "./coloring";
import { buildLayout, encodeColor, encodePresence, FADE, homeArea, presenceEncoding } from "./galaxyModel";
import { PlayheadMask, passMask } from "./gate";
import { conv, DAY0, snapshot } from "./testSnapshot";

describe("buildLayout", () => {
  it("skips conversations without xy and fits the layout bounds inside [-1, 1]", () => {
    const s = snapshot([conv({ xy: [10, 0] }), conv({ xy: null }), conv({ xy: [30, 10] })], {
      layout: { method: "t", bounds: [10, 0, 30, 10] },
    });
    const l = buildLayout(s);
    expect(Array.from(l.convOf)).toEqual([0, 2]);
    expect(Array.from(l.pointOf)).toEqual([0, -1, 1]);
    for (const v of [...l.x, ...l.y]) expect(Math.abs(v)).toBeLessThanOrEqual(1);
    // aspect is preserved: the wider axis spans the same width either side of centre
    expect(l.x[0]).toBeCloseTo(-l.x[1]);
  });
});

describe("homeArea", () => {
  it("frames the bulk of the points with padding and ignores a lone outlier", () => {
    // 200 points on [0, 0.5] × [-0.5, 0], plus one far outlier
    const n = 200;
    const x = new Float32Array(n + 1);
    const y = new Float32Array(n + 1);
    for (let i = 0; i < n; i++) [x[i], y[i]] = [(i / (n - 1)) * 0.5, -((i * 7) % n) / (n - 1) * 0.5];
    [x[n], y[n]] = [0.99, 0.99];
    const a = homeArea(x, y);
    expect(a.x + a.width).toBeLessThan(0.6);
    expect(a.y + a.height).toBeLessThan(0.1);
    // the bulk sits inside the frame with a margin either side
    expect(a.x).toBeLessThan(0.01);
    expect(a.x + a.width).toBeGreaterThan(0.5);
    expect(a.y).toBeLessThan(-0.49);
    expect(a.y + a.height).toBeGreaterThan(0);
  });

  it("gives a single point a non-degenerate frame centred on it", () => {
    const a = homeArea(Float32Array.of(0.3), Float32Array.of(-0.2));
    expect(a.width).toBeGreaterThan(0);
    expect(a.height).toBeGreaterThan(0);
    expect(a.x + a.width / 2).toBeCloseTo(0.3);
    expect(a.y + a.height / 2).toBeCloseTo(-0.2);
  });
});

describe("encodePresence", () => {
  const s = snapshot([
    conv({ started_ms: DAY0, turns: 100, activity: [[0, 10], [5, 90]], repo: 0 }),
    conv({ started_ms: DAY0, turns: 4, activity: [[0, 4]], repo: 1 }),
    conv({ started_ms: DAY0 + 10 * DAY_MS, turns: 4, activity: [[10, 4]], repo: 0 }),
  ], { repos: [{ key: "a", label: "a" }, { key: "b", label: "b" }] });
  const layout = buildLayout(s);
  const at = (cursor: number, filters = NO_FILTERS) => {
    const gate = new PlayheadMask((i) => s.conversations[i].started_ms, 3, passMask(s, filters));
    gate.seek(cursor);
    return Array.from(encodePresence(s, layout, gate.exists, gate.pass, Math.floor((cursor - DAY0) / DAY_MS), new Float32Array(3)));
  };

  it("hides not-yet-started conversations and sizes the rest by turns so far", () => {
    const [big, small, future] = at(DAY0 + 2 * DAY_MS);
    expect(future).toBe(0);
    expect(big).toBeGreaterThan(FADE);
    expect(small).toBeGreaterThan(FADE);
    expect(big).toBeGreaterThan(small);
  });

  it("grows a conversation as the playhead passes its activity", () => {
    expect(at(DAY0 + 6 * DAY_MS)[0]).toBeGreaterThan(at(DAY0 + 2 * DAY_MS)[0]);
    expect(at(DAY0 + 20 * DAY_MS)[0]).toBeCloseTo(1);
  });

  it("keeps filtered-out but started conversations as ghosts at FADE", () => {
    const [, ghost, future] = at(DAY0 + 2 * DAY_MS, { ...NO_FILTERS, repos: ["a"] });
    expect(ghost).toBeCloseTo(FADE);
    expect(future).toBe(0);
  });
});

describe("presenceEncoding", () => {
  it("never encodes a zero size (regl-scatterplot divides by sizes[0]) and is invisible at w = 0", () => {
    const { sizes, opacities } = presenceEncoding(3, 18, 0.9);
    expect(Math.min(...sizes)).toBe(3);
    expect(sizes.at(-1)).toBe(18);
    expect(opacities[0]).toBe(0);
    expect(opacities.at(-1)).toBeCloseTo(0.9);
  });
});

describe("colorCoding", () => {
  const community = (label: string, size: number) => ({ label, blurb: "", tags: [], size, centroid_xy: [0, 0] as [number, number] });
  // snapshots register colour slots when built, so build them after the reset
  const firstSnapshot = () =>
    snapshot(
      [
        conv({ community: 1 }),
        conv({ community: 1 }),
        conv({ community: 1 }),
        conv({ community: 0 }),
        conv({ community: null }),
        conv({ community: 0, xy: null }),
      ],
      { communities: [community("small", 2), community("big", 3)] },
    );
  beforeEach(() => resetColorSlots());

  it("orders the legend by size and gives the largest category the first colour slot on first sight", () => {
    const c = colorCoding(firstSnapshot(), "community", "dark");
    expect(c.legend.map((e) => [e.label, e.count])).toEqual([
      ["big", 3],
      ["small", 2],
      ["none", 1],
    ]);
    // community 1 is larger, so it takes CVD-safe slot 0 even though its index is 1
    expect(c.colors[1]).toEqual(communityColor(0, "dark"));
    expect(c.colors[0]).toEqual(communityColor(1, "dark"));
    expect(c.legend[0].key).toBe("1");
  });

  it("keeps a community's colour when a later snapshot reorders sizes; new communities take the lowest free slot", () => {
    const first = colorCoding(firstSnapshot(), "community", "dark");
    const grown = snapshot(
      [...Array.from({ length: 5 }, () => conv({ community: 0 })), conv({ community: 1 }), conv({ community: 2 })],
      { communities: [community("small", 5), community("big", 1), community("new", 1)] },
    );
    const later = colorCoding(grown, "community", "dark");
    expect(later.legend.map((e) => e.label)).toEqual(["small", "big", "new"]);
    expect(later.colors[0]).toEqual(first.colors[0]);
    expect(later.colors[1]).toEqual(first.colors[1]);
    expect(later.colors[2]).toEqual(communityColor(2, "dark"));
  });

  it("paints filtered-out points with the ghost category", () => {
    const s = firstSnapshot();
    const layout = buildLayout(s);
    const c = colorCoding(s, "community", "dark");
    const pass = passMask(s, { ...NO_FILTERS, communities: [1] });
    const z = Array.from(encodeColor(c, layout, pass, new Float32Array(layout.convOf.length)));
    expect(z).toEqual([1, 1, 1, c.ghost, c.ghost]);
  });
});
