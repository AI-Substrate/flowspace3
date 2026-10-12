/** Small React helpers shared by the Chronos views (inside the Chronos fence). */
import { type CSSProperties, type RefObject, useEffect, useRef, useState } from "react";
import { type Theme, getTheme, onThemeChange } from "../../design/theme";

/** Element size via ResizeObserver; re-renders only when the size changes. */
export function useSize<T extends HTMLElement>(): [RefObject<T | null>, { width: number; height: number }] {
  const ref = useRef<T>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      setSize((s) => (s.width === width && s.height === height ? s : { width, height }));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return [ref, size];
}

/**
 * The current theme as React state. Palette functions read the theme when
 * called, so SVG/canvas colours must be recomputed when it changes: list the
 * returned value in the deps of anything that calls palette.ts.
 */
export function useTheme(): Theme {
  const [theme, setTheme] = useState<Theme>(getTheme);
  useEffect(() => onThemeChange(setTheme), []);
  return theme;
}

/**
 * Keyboard step through `n` items, for roving focus: the next index for a
 * navigation key, or null when the key is not one. Axis "y" uses Up/Down,
 * PageUp/PageDown (10) and Home/End; axis "x" uses Left/Right only, so a
 * vertical list leaves Left/Right to the shell's playhead keys. Callers
 * `preventDefault()` on non-null so the shell does not also act.
 */
export function stepIndex(key: string, cur: number, n: number, axis: "x" | "y" = "y"): number | null {
  const steps: Record<string, number> =
    axis === "y"
      ? { ArrowDown: 1, ArrowUp: -1, PageDown: 10, PageUp: -10, Home: -Infinity, End: Infinity }
      : { ArrowRight: 1, ArrowLeft: -1 };
  const d = steps[key];
  if (d === undefined || n === 0) return null;
  return Math.max(0, Math.min(n - 1, cur + d));
}

export const stageStyle: CSSProperties = {
  position: "absolute",
  inset: 0,
  display: "flex",
  flexDirection: "column",
  background: "var(--bg)",
  color: "var(--ink)",
};
