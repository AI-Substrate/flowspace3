/**
 * Shareable links: `#/<view>?sel=<guid>&t=<ms>&f=<json>`.
 * - Read once at startup and applied when the first snapshot arrives (a guid
 *   that is not in it is dropped). Back/forward and hand-edited hashes apply live.
 * - Written with replaceState, debounced; `t` only while paused and not at the
 *   end of the range (the end is the default "show everything" state).
 */
import { atlas } from "../core/store";
import { type Filters, NO_FILTERS } from "../core/semantics";

interface HashState {
  view?: string;
  sel?: string;
  t?: number;
  filters?: Partial<Filters>;
}

export function parseHash(hash: string): HashState {
  const m = /^#\/([^?]*)(?:\?(.*))?$/.exec(hash);
  if (!m) return {};
  const q = new URLSearchParams(m[2] ?? "");
  const out: HashState = {};
  if (m[1]) out.view = decodeURIComponent(m[1]);
  const sel = q.get("sel");
  if (sel) out.sel = sel;
  const t = Number(q.get("t"));
  if (q.has("t") && Number.isFinite(t)) out.t = t;
  const f = q.get("f");
  if (f) {
    try {
      const raw = JSON.parse(f) as Record<string, unknown>;
      const filters: Partial<Filters> = {};
      for (const key of ["repos", "harnesses", "seats", "kinds"] as const) {
        const v = raw[key];
        if (Array.isArray(v)) (filters as Record<string, unknown>)[key] = v.filter((x) => typeof x === "string");
      }
      if (Array.isArray(raw.communities)) filters.communities = raw.communities.filter((x): x is number => Number.isInteger(x));
      if (typeof raw.query === "string") filters.query = raw.query;
      out.filters = filters;
    } catch {
      // a mangled link still opens the view
    }
  }
  return out;
}

export function formatHash(s: { view: string; selected: string | null; filters: Filters; cursorMs: number | null }): string {
  const q = new URLSearchParams();
  if (s.selected) q.set("sel", s.selected);
  if (s.cursorMs !== null) q.set("t", String(Math.round(s.cursorMs)));
  const f: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(s.filters)) {
    if (Array.isArray(v) ? v.length : v !== NO_FILTERS[k as keyof Filters]) f[k] = v;
  }
  if (Object.keys(f).length) q.set("f", JSON.stringify(f));
  const qs = q.toString();
  return `#/${encodeURIComponent(s.view)}${qs ? `?${qs}` : ""}`;
}

function apply(h: HashState): void {
  const s = atlas.getState();
  if (h.view) s.setView(h.view);
  s.setFilters({ ...NO_FILTERS, ...h.filters });
  const snap = s.snapshot;
  if (!snap) return;
  s.select(h.sel && snap.guidIndex.has(h.sel) ? h.sel : null);
  const [start, end] = s.time.range;
  s.setTime({ cursorMs: h.t === undefined ? end : Math.min(end, Math.max(start, h.t)), playing: false });
}

export function startHashSync(): () => void {
  let applying = false;
  const applyNow = (h: HashState) => {
    applying = true;
    apply(h);
    applying = false;
  };

  const initial = parseHash(location.hash);
  if (initial.view) atlas.getState().setView(initial.view);
  let unsubFirst = () => {};
  if (atlas.getState().snapshot) applyNow(initial);
  else {
    unsubFirst = atlas.subscribe(
      (s) => s.snapshot,
      (snap) => {
        if (!snap) return;
        unsubFirst();
        applyNow(initial);
      },
    );
  }

  let timer = 0;
  const write = () => {
    timer = 0;
    const s = atlas.getState();
    if (!s.snapshot) return;
    const atEnd = s.time.cursorMs >= s.time.range[1];
    const next = formatHash({
      view: s.view,
      selected: s.selected,
      filters: s.filters,
      cursorMs: s.time.playing || atEnd ? null : s.time.cursorMs,
    });
    if (next !== location.hash) history.replaceState(null, "", next);
  };
  const unsub = atlas.subscribe(
    (s) => [s.view, s.selected, s.filters, s.time.playing ? -1 : s.time.cursorMs, s.snapshot] as const,
    () => {
      if (applying) return;
      clearTimeout(timer);
      timer = window.setTimeout(write, 250);
    },
    { equalityFn: (a, b) => a.every((v, i) => v === b[i]) },
  );

  const onHash = () => applyNow(parseHash(location.hash));
  addEventListener("hashchange", onHash);
  return () => {
    unsubFirst();
    unsub();
    clearTimeout(timer);
    removeEventListener("hashchange", onHash);
  };
}
