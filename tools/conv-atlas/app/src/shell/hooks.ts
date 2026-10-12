import { useEffect, useState, useSyncExternalStore } from "react";
import { atlas, type AtlasState } from "../core/store";
import { getTheme, onThemeChange, type Theme } from "../design/theme";

/** The current theme; components that bake palette colours into render re-render on change. */
export function useTheme(): Theme {
  return useSyncExternalStore(onThemeChange, getTheme);
}

/** Re-renders every `ms` so relative times ("3 min ago") stay true. */
export function useNow(ms = 15_000): number {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), ms);
    return () => clearInterval(id);
  }, [ms]);
  return now;
}

/**
 * A store value for React UI that would otherwise re-render every frame during
 * playback (counts, readouts). Trailing-edge throttle; the last value always lands.
 */
export function useThrottledAtlas<T>(selector: (s: AtlasState) => T, ms = 200): T {
  const [value, setValue] = useState(() => selector(atlas.getState()));
  useEffect(() => {
    let timer = 0;
    let last = 0;
    const unsub = atlas.subscribe(selector, (v) => {
      clearTimeout(timer);
      const wait = Math.max(0, last + ms - performance.now());
      timer = window.setTimeout(() => {
        last = performance.now();
        setValue(v);
      }, wait);
    });
    setValue(selector(atlas.getState()));
    return () => {
      unsub();
      clearTimeout(timer);
    };
    // the selector is expected to be a stable module-level function
  }, [ms]);
  return value;
}

