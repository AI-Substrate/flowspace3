/**
 * Theme state: dark first, light on request or when the OS prefers it.
 * `<html data-theme>` drives tokens.css; `onThemeChange` lets WebGL/canvas
 * views re-read palette.ts colours (they are theme-aware).
 */
export type Theme = "dark" | "light";

const KEY = "conv-atlas.theme";
const listeners = new Set<(t: Theme) => void>();

function systemTheme(): Theme {
  return typeof matchMedia === "function" && matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

function storedTheme(): Theme | null {
  try {
    const v = localStorage.getItem(KEY);
    return v === "dark" || v === "light" ? v : null;
  } catch {
    return null;
  }
}

let current: Theme = typeof document === "undefined" ? "dark" : (storedTheme() ?? systemTheme());

export function getTheme(): Theme {
  return current;
}

function apply(t: Theme): void {
  const changed = t !== current;
  current = t;
  document.documentElement.dataset.theme = t;
  document.documentElement.style.colorScheme = t;
  if (changed) for (const cb of listeners) cb(t);
}

/** An explicit choice sticks across reloads; `null` returns to following the OS. */
export function setTheme(t: Theme | null): void {
  try {
    if (t) localStorage.setItem(KEY, t);
    else localStorage.removeItem(KEY);
  } catch {
    // private mode: the choice lasts for this page only
  }
  apply(t ?? systemTheme());
}

export function onThemeChange(cb: (t: Theme) => void): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

/** Call once at startup: stamps the attribute and follows OS changes until the user picks. */
export function initTheme(): void {
  apply(current);
  matchMedia("(prefers-color-scheme: light)").addEventListener("change", () => {
    if (!storedTheme()) apply(systemTheme());
  });
}
