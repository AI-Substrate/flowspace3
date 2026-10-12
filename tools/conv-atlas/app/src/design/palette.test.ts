import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  ageRamp,
  CATEGORICAL_SIZE,
  CHROME,
  type ChromeRole,
  categorical,
  chrome,
  dimmed,
  harnessColor,
  markStroke,
  type RGBA,
} from "./palette";
import type { Theme } from "./theme";

const THEMES: Theme[] = ["dark", "light"];

const toLin = (c: number) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
const luminance = (c: RGBA) => 0.2126 * toLin(c[0]) + 0.7152 * toLin(c[1]) + 0.0722 * toLin(c[2]);
function contrast(a: RGBA, b: RGBA): number {
  const [x, y] = [luminance(a), luminance(b)];
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}

// Machado et al. 2009, severity 1.0, in linear RGB.
const CVD: Record<string, number[][] | null> = {
  normal: null,
  protan: [
    [0.152286, 1.052583, -0.204868],
    [0.114503, 0.786281, 0.099216],
    [-0.003882, -0.048116, 1.051998],
  ],
  deutan: [
    [0.367322, 0.860646, -0.227968],
    [0.280085, 0.672501, 0.047413],
    [-0.01182, 0.04294, 0.968881],
  ],
  tritan: [
    [1.255528, -0.076749, -0.178779],
    [-0.078411, 0.930809, 0.147602],
    [0.004733, 0.691367, 0.3039],
  ],
};

function oklab(c: RGBA, m: number[][] | null): [number, number, number] {
  let v = [toLin(c[0]), toLin(c[1]), toLin(c[2])];
  if (m) v = m.map((r) => Math.min(1, Math.max(0, r[0] * v[0] + r[1] * v[1] + r[2] * v[2])));
  const l = Math.cbrt(0.4122214708 * v[0] + 0.5363325363 * v[1] + 0.0514459929 * v[2]);
  const mm = Math.cbrt(0.2119034982 * v[0] + 0.6806995451 * v[1] + 0.1073969566 * v[2]);
  const s = Math.cbrt(0.0883024619 * v[0] + 0.2817188376 * v[1] + 0.6299787005 * v[2]);
  return [
    0.2104542553 * l + 0.793617785 * mm - 0.0040720468 * s,
    1.9779984951 * l - 2.428592205 * mm + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * mm - 0.808675766 * s,
  ];
}

/** Smallest pairwise OKLab distance in the set, under one vision type. */
function minDistance(colors: RGBA[], vision: keyof typeof CVD): number {
  const pts = colors.map((c) => oklab(c, CVD[vision]));
  let min = Infinity;
  for (let i = 0; i < pts.length; i++)
    for (let j = i + 1; j < pts.length; j++) min = Math.min(min, Math.hypot(...pts[i].map((v, k) => v - pts[j][k])));
  return min;
}

describe("tokens.css mirrors palette CHROME", () => {
  const cssText = readFileSync(new URL("./tokens.css", import.meta.url), "utf8");
  const blocks: Record<Theme, string> = {
    dark: cssText.slice(cssText.indexOf('[data-theme="dark"]'), cssText.indexOf('[data-theme="light"]')),
    light: cssText.slice(cssText.indexOf('[data-theme="light"]'), cssText.indexOf(":root {", cssText.indexOf('[data-theme="light"]'))),
  };
  for (const theme of THEMES) {
    it(`${theme}: every chrome role has the same OKLCH value in CSS`, () => {
      for (const [role, [L, C, h]] of Object.entries(CHROME[theme])) {
        const m = blocks[theme].match(new RegExp(`--${role}: oklch\\(([\\d.]+) ([\\d.]+) ([\\d.]+)\\);`));
        expect(m, `--${role} missing in ${theme}`).not.toBeNull();
        expect(m?.slice(1).map(Number), `--${role} (${theme})`).toEqual([L, C, h]);
      }
    });
  }
});

describe("chrome contrast (WCAG)", () => {
  for (const theme of THEMES) {
    const c = (r: ChromeRole) => chrome(r, theme);
    it(`${theme}: text roles are readable on every surface`, () => {
      for (const surface of ["bg", "panel", "raised"] as const) {
        expect(contrast(c("ink"), c(surface))).toBeGreaterThanOrEqual(7);
        expect(contrast(c("ink-2"), c(surface))).toBeGreaterThanOrEqual(4.5);
        expect(contrast(c("muted"), c(surface))).toBeGreaterThanOrEqual(4.5);
      }
    });
    it(`${theme}: accent and status colours hold 3:1 as marks; filled accent buttons hold 4.5:1 text`, () => {
      for (const role of ["accent", "ok", "warn", "danger", "line-strong"] as const) {
        expect(contrast(c(role), c("bg")), role).toBeGreaterThanOrEqual(role === "line-strong" ? 1.9 : 3);
      }
      expect(contrast(c("on-accent"), c("accent-strong"))).toBeGreaterThanOrEqual(4.5);
    });
  }
});

describe("categorical", () => {
  for (const theme of THEMES) {
    const all = Array.from({ length: CATEGORICAL_SIZE }, (_, i) => categorical(i, theme));
    const core = all.slice(0, 8);

    // WCAG 1.4.11: a mark needs 3:1 from its fill, or from its outline where the fill is lighter.
    it(`${theme}: every hue is perceivable as a mark against the background`, () => {
      const bg = chrome("bg", theme);
      const stroke = markStroke(theme);
      for (const [i, col] of all.entries()) {
        const fill = contrast(col, bg);
        if (stroke[3] > 0) {
          expect(fill, `hue ${i}`).toBeGreaterThanOrEqual(2);
          expect(contrast(stroke, bg)).toBeGreaterThanOrEqual(3);
        } else {
          expect(fill, `hue ${i}`).toBeGreaterThanOrEqual(3);
        }
      }
    });
    // OKLab ΔE ≈ 0.02 is a just-noticeable difference.
    it(`${theme}: the first 8 stay apart for normal, protan, deutan and tritan vision`, () => {
      expect(minDistance(core, "normal")).toBeGreaterThan(0.1);
      for (const v of ["protan", "deutan", "tritan"] as const) expect(minDistance(core, v), v).toBeGreaterThan(0.03);
    });
    it(`${theme}: all 24 stay at least two JNDs apart for normal vision`, () => {
      expect(minDistance(all, "normal")).toBeGreaterThan(0.045);
    });
    it(`${theme}: ghosts (dimmed) stay visible but recede below every hue`, () => {
      const bg = chrome("bg", theme);
      const ghost = contrast(dimmed(theme), bg);
      expect(ghost).toBeGreaterThanOrEqual(1.65);
      expect(ghost).toBeLessThan(Math.min(...all.map((col) => contrast(col, bg))) * 0.8);
    });
  }

  it("wraps and accepts negative indices", () => {
    expect(categorical(CATEGORICAL_SIZE + 2, "dark")).toEqual(categorical(2, "dark"));
    expect(categorical(-1, "dark")).toEqual(categorical(CATEGORICAL_SIZE - 1, "dark"));
  });

  it("returns a copy, so a view setting alpha cannot repaint other marks", () => {
    const a = categorical(0, "dark");
    a[3] = 0.2;
    expect(categorical(0, "dark")[3]).toBe(1);
  });
});

describe("harnessColor", () => {
  it("is case-insensitive for known harnesses and stable for unknown ones", () => {
    expect(harnessColor("Claude", "dark")).toEqual(harnessColor("claude", "dark"));
    expect(harnessColor("some-new-harness", "dark")).toEqual(harnessColor("some-new-harness", "dark"));
  });
  it("keeps unknown harnesses out of the 8 core colours used by the known ones", () => {
    const core = Array.from({ length: 8 }, (_, i) => categorical(i, "dark").join());
    for (const name of ["aider", "zed", "goose", "amp", "cline", "roo"]) {
      expect(core).not.toContain(harnessColor(name, "dark").join());
    }
  });
});

describe("ageRamp", () => {
  for (const theme of THEMES) {
    it(`${theme}: newer is always further from the background (monotone luminance)`, () => {
      const dir = theme === "dark" ? 1 : -1;
      let prev = luminance(ageRamp(0, theme));
      for (let k = 1; k <= 64; k++) {
        const cur = luminance(ageRamp(k / 64, theme));
        expect(dir * (cur - prev)).toBeGreaterThanOrEqual(-1e-9);
        prev = cur;
      }
    });
  }
  it("clamps out-of-range and non-finite input", () => {
    expect(ageRamp(-3, "dark")).toEqual(ageRamp(0, "dark"));
    expect(ageRamp(7, "dark")).toEqual(ageRamp(1, "dark"));
    expect(ageRamp(Number.NaN, "dark")).toEqual(ageRamp(0, "dark"));
  });
});
