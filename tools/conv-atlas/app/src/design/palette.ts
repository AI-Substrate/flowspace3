/**
 * The cross-view colour contract. Every colour a view draws comes from here.
 *
 * - Colours are authored in OKLCH and returned as `[r, g, b, a]` sRGB floats
 *   0..1 (WebGL-ready); `css()` turns one into a canvas/DOM string.
 * - Theme-aware: marks get darker on the light theme so they keep contrast.
 *   Imperative views re-read colours inside `onThemeChange` (design/theme.ts).
 * - Chrome colours mirror tokens.css (`--bg`, `--ink`, ...); palette.test.ts
 *   keeps the two identical, so a canvas background matches the DOM around it.
 */
import type { Conversation, InsightKind, SessionTurn } from "../contract/types";
import { getTheme, type Theme } from "./theme";

export type RGBA = [number, number, number, number];
/** [lightness 0..1, chroma, hue degrees] */
export type Oklch = readonly [number, number, number];

// ---------------------------------------------------------------- conversion

function oklchToLinear([L, C, h]: Oklch): [number, number, number] {
  const a = C * Math.cos((h * Math.PI) / 180);
  const b = C * Math.sin((h * Math.PI) / 180);
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
}

const inGamut = (rgb: number[]) => rgb.every((c) => c >= -1e-4 && c <= 1 + 1e-4);

/** OKLCH → sRGB floats. Out-of-gamut colours keep L and h and lose chroma until they fit. */
export function oklch(color: Oklch, alpha = 1): RGBA {
  let lin = oklchToLinear(color);
  if (!inGamut(lin)) {
    let lo = 0;
    let hi = color[1];
    for (let i = 0; i < 18; i++) {
      const mid = (lo + hi) / 2;
      if (inGamut(oklchToLinear([color[0], mid, color[2]]))) lo = mid;
      else hi = mid;
    }
    lin = oklchToLinear([color[0], lo, color[2]]);
  }
  const gamma = (c: number) => {
    const v = Math.min(1, Math.max(0, c));
    return v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055;
  };
  return [gamma(lin[0]), gamma(lin[1]), gamma(lin[2]), alpha];
}

/** `rgba(r,g,b,a)` for canvas 2D and inline styles. */
export function css(c: RGBA): string {
  return `rgba(${Math.round(c[0] * 255)},${Math.round(c[1] * 255)},${Math.round(c[2] * 255)},${+c[3].toFixed(3)})`;
}

export function withAlpha(c: RGBA, a: number): RGBA {
  return [c[0], c[1], c[2], a];
}

// ---------------------------------------------------------------- chrome

export type ChromeRole =
  | "bg"
  | "panel"
  | "raised"
  | "line"
  | "line-strong"
  | "ink"
  | "ink-2"
  | "muted"
  | "faint"
  | "accent"
  | "accent-strong"
  | "on-accent"
  | "ok"
  | "warn"
  | "danger";

/** Source of truth for chrome colours; tokens.css carries the same values. */
export const CHROME: Record<Theme, Record<ChromeRole, Oklch>> = {
  dark: {
    bg: [0.165, 0, 0],
    panel: [0.198, 0.005, 270],
    raised: [0.238, 0.007, 270],
    line: [0.3, 0.008, 270],
    "line-strong": [0.4, 0.01, 270],
    ink: [0.95, 0.004, 270],
    "ink-2": [0.8, 0.008, 270],
    muted: [0.68, 0.01, 270],
    faint: [0.5, 0.01, 270],
    accent: [0.72, 0.17, 355],
    "accent-strong": [0.56, 0.2, 355],
    "on-accent": [1, 0, 0],
    ok: [0.76, 0.14, 160],
    warn: [0.84, 0.14, 85],
    danger: [0.7, 0.18, 28],
  },
  light: {
    bg: [1, 0, 0],
    panel: [0.972, 0.003, 270],
    raised: [0.945, 0.005, 270],
    line: [0.9, 0.006, 270],
    "line-strong": [0.78, 0.01, 270],
    ink: [0.22, 0.012, 270],
    "ink-2": [0.38, 0.012, 270],
    muted: [0.5, 0.012, 270],
    faint: [0.7, 0.01, 270],
    accent: [0.54, 0.2, 355],
    "accent-strong": [0.5, 0.2, 355],
    "on-accent": [1, 0, 0],
    ok: [0.52, 0.13, 160],
    warn: [0.6, 0.13, 70],
    danger: [0.55, 0.2, 28],
  },
};

export function chrome(role: ChromeRole, theme: Theme = getTheme()): RGBA {
  return oklch(CHROME[theme][role]);
}

/** The "now" colour: playhead, selection, primary action. */
export function accent(theme: Theme = getTheme()): RGBA {
  return oklch(CHROME[theme].accent);
}

/** t01 = 0 is the background, 1 is ink; for grids, edges and de-emphasised marks. */
export function neutral(t01: number, theme: Theme = getTheme()): RGBA {
  const a = CHROME[theme].bg;
  const b = CHROME[theme].ink;
  const t = Math.min(1, Math.max(0, t01));
  return oklch([a[0] + (b[0] - a[0]) * t, 0.006 * t, 270]);
}

/**
 * Marks that fail the filters or do not exist yet at the playhead: about 2:1 on dark and
 * 1.7:1 on light, so ghosts still read as context, but always under the weakest categorical colour.
 */
export function dimmed(theme: Theme = getTheme()): RGBA {
  return neutral(theme === "dark" ? 0.3 : 0.22, theme);
}

/** Outline for point marks: none on dark; a hairline on light, where some hues sit near 3:1 on white. */
export function markStroke(theme: Theme = getTheme()): RGBA {
  return theme === "dark" ? [0, 0, 0, 0] : neutral(0.55, theme);
}

// ---------------------------------------------------------------- categorical

/**
 * 24 hues for the dark theme. Order matters: the first 8 are an Okabe–Ito-style
 * core chosen to stay apart under protan/deutan/tritan simulation; the next 16
 * were picked farthest-point from the core in OKLab, CVD-weighted. Small sets
 * (harnesses, kinds, most community counts) therefore get the safest colours.
 */
export const CATEGORICAL_DARK: readonly Oklch[] = [
  [0.76, 0.12, 235],
  [0.79, 0.15, 68],
  [0.73, 0.13, 168],
  [0.7, 0.14, 318],
  [0.91, 0.15, 105],
  [0.66, 0.18, 35],
  [0.62, 0.16, 262],
  [0.86, 0.07, 290],
  [0.6, 0.16, 135],
  [0.86, 0.16, 155],
  [0.66, 0.112, 210],
  [0.6, 0.16, 335],
  [0.72, 0.143, 275],
  [0.79, 0.16, 335],
  [0.72, 0.145, 95],
  [0.6, 0.16, 10],
  [0.6, 0.111, 175],
  [0.79, 0.16, 135],
  [0.86, 0.098, 65],
  [0.6, 0.16, 300],
  [0.79, 0.131, 305],
  [0.86, 0.16, 125],
  [0.79, 0.136, 185],
  [0.86, 0.146, 190],
];

/**
 * Light theme: same hues and order, lightness pulled into 0.48..0.68 so hues stay
 * vivid on white; `markStroke()` supplies the 3:1 edge the lightest ones lack.
 */
export const CATEGORICAL_LIGHT: readonly Oklch[] = CATEGORICAL_DARK.map(
  ([L, C, h]) => [0.48 + (L - 0.6) * (0.2 / 0.31), C, h] as const,
);

const categoricalCache: Record<Theme, RGBA[]> = {
  dark: CATEGORICAL_DARK.map((c) => oklch(c)),
  light: CATEGORICAL_LIGHT.map((c) => oklch(c)),
};

export const CATEGORICAL_SIZE = CATEGORICAL_DARK.length;

/** Distinct colour i (wraps after 24). Returns a fresh array, safe to mutate. */
export function categorical(i: number, theme: Theme = getTheme()): RGBA {
  const n = CATEGORICAL_SIZE;
  return [...categoricalCache[theme][((Math.floor(i) % n) + n) % n]] as RGBA;
}

/** Community colour; `null` (unclustered) is a quiet neutral. */
export function communityColor(i: number | null, theme: Theme = getTheme()): RGBA {
  return i === null ? neutral(0.45, theme) : categorical(i, theme);
}

/** Known harnesses keep a fixed colour across snapshots; others hash into the extended set. */
const HARNESS_SLOT: Record<string, number> = {
  claude: 1,
  omp: 0,
  codex: 2,
  copilot: 6,
  pi: 3,
  gemini: 4,
  opencode: 5,
  cursor: 7,
};

export function harnessColor(name: string | null, theme: Theme = getTheme()): RGBA {
  if (name === null) return neutral(0.45, theme);
  const slot = HARNESS_SLOT[name.toLowerCase()];
  if (slot !== undefined) return categorical(slot, theme);
  let h = 2166136261;
  for (let i = 0; i < name.length; i++) h = Math.imul(h ^ name.charCodeAt(i), 16777619);
  return categorical(8 + ((h >>> 0) % (CATEGORICAL_SIZE - 8)), theme);
}

const KIND_SLOT: Record<Conversation["kind"] | InsightKind, number> = {
  session: 0,
  subagent: 1,
  finding: 2,
  question: 4,
  anomaly: 5,
  story: 3,
};

/** Conversation kinds (session, subagent) and insight kinds (finding, question, anomaly, story). */
export function kindColor(kind: Conversation["kind"] | InsightKind, theme: Theme = getTheme()): RGBA {
  return categorical(KIND_SLOT[kind], theme);
}

/**
 * Who spoke a turn. Human = warm, agent = cool, peer agents = violet;
 * a system-sourced "human" turn (injected prompt) is neutral.
 */
export function roleColor(role: SessionTurn["role"], source: SessionTurn["source"], theme: Theme = getTheme()): RGBA {
  if (source === "peer") return categorical(3, theme);
  if (role === "human") return source === "human" ? categorical(1, theme) : neutral(0.55, theme);
  return categorical(0, theme);
}

export type TurnClass = "work" | "tool_call" | "tool_result" | "retrieval";

/** Core slots kept apart from roleColor's (0, 1, 3) so a composition and a speaker bar can sit side by side. */
const TURN_CLASS_SLOT: Record<TurnClass, number> = { work: 2, tool_call: 6, tool_result: 7, retrieval: 4 };

/** What a turn was: agent work, a tool call, its result, or a retrieval. */
export function turnClassColor(cls: TurnClass, theme: Theme = getTheme()): RGBA {
  return categorical(TURN_CLASS_SLOT[cls], theme);
}

// ---------------------------------------------------------------- sequential

/** Old → new. Monotone lightness, so it still reads in greyscale and under CVD. */
const AGE_STOPS: Record<Theme, readonly Oklch[]> = {
  dark: [
    [0.36, 0.09, 275],
    [0.62, 0.11, 205],
    [0.93, 0.14, 95],
  ],
  light: [
    [0.9, 0.05, 95],
    [0.64, 0.11, 195],
    [0.36, 0.14, 280],
  ],
};

const AGE_LUT_SIZE = 256;
const ageLut: Partial<Record<Theme, RGBA[]>> = {};

function buildAgeLut(theme: Theme): RGBA[] {
  const stops = AGE_STOPS[theme];
  const lut: RGBA[] = [];
  for (let k = 0; k < AGE_LUT_SIZE; k++) {
    const t = (k / (AGE_LUT_SIZE - 1)) * (stops.length - 1);
    const i = Math.min(stops.length - 2, Math.floor(t));
    const f = t - i;
    const [a, b] = [stops[i], stops[i + 1]];
    let dh = b[2] - a[2];
    if (dh > 180) dh -= 360;
    if (dh < -180) dh += 360;
    lut.push(oklch([a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + dh * f]));
  }
  return lut;
}

/** t01 = 0 oldest, 1 newest. LUT-backed: cheap enough to call per point per frame. */
export function ageRamp(t01: number, theme: Theme = getTheme()): RGBA {
  const lut = (ageLut[theme] ??= buildAgeLut(theme));
  const k = Math.round(Math.min(1, Math.max(0, Number.isFinite(t01) ? t01 : 0)) * (AGE_LUT_SIZE - 1));
  return [...lut[k]] as RGBA;
}
