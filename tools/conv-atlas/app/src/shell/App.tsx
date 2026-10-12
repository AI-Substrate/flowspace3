/**
 * The shell: top bar, filter bar, stage (the active view), right panel
 * (details, reader, insights) and the playback transport.
 */
import "@fontsource-variable/recursive/mono.css";
import "../design/tokens.css";
import "../design/primitives";
import "./shell.css";
import { type ReactNode, useEffect } from "react";
import { VIEWS } from "../core/registry";
import { WATCH_INTERVAL_MS } from "../core/runtime";
import { atlas, useAtlas } from "../core/store";
import { EmptyState } from "../design/primitives";
import { initTheme } from "../design/theme";
import { CommandPalette } from "./CommandPalette";
import { Detail } from "./Detail";
import { FilterBar } from "./FilterBar";
import { fmt } from "./format";
import { startHashSync } from "./hashState";
import { InsightsPanel } from "./Insights";
import { KeysOverlay, useGlobalKeys } from "./Keys";
import { ReaderPanel } from "./Reader";
import { TopBar } from "./TopBar";
import { Transport } from "./Transport";
import { useUi } from "./ui";
import { ViewFrame } from "./ViewFrame";

initTheme();

/** Panel follows the store: selecting opens details, focusing a turn opens the reader. */
function usePanelRules(): void {
  useEffect(() => {
    const ui = useUi.getState;
    const offSel = atlas.subscribe(
      (s) => s.selected,
      (sel) => {
        const p = ui().panel;
        if (sel && p !== "reader") ui().setPanel("detail");
        if (!sel && (p === "detail" || p === "reader")) ui().setPanel(null);
      },
    );
    const offFocus = atlas.subscribe(
      (s) => s.focusTurn,
      (f) => {
        if (f) ui().setPanel("reader");
      },
    );
    const offActive = atlas.subscribe(
      (s) => s.activeInsight,
      (id) => {
        if (id) ui().setPanel("insights");
      },
    );
    // explain a snapshot swap: what changed
    const offSnap = atlas.subscribe(
      (s) => s.snapshot,
      (next, prev) => {
        if (!next || !prev || next.id === prev.id) return;
        const dc = next.conversations.length - prev.conversations.length;
        const dt = (next.meta.stats.turns_total ?? 0) - (prev.meta.stats.turns_total ?? 0);
        const delta = (n: number, what: string) => (n === 0 ? null : `${n > 0 ? "+" : "−"}${fmt.count(Math.abs(n))} ${what}`);
        const changes = [delta(dc, "conversations"), delta(dt, "turns")].filter(Boolean).join(", ");
        ui().toast(`Now showing the snapshot exported ${fmt.dateTime(next.meta.created_ms)}${changes ? `: ${changes}` : ""}.`);
      },
    );
    return () => {
      offSel();
      offFocus();
      offActive();
      offSnap();
    };
  }, []);
}

function Toasts() {
  const toasts = useUi((s) => s.toasts);
  return (
    <div className="sh-toasts" role="status" aria-live="polite">
      {toasts.map((t) => (
        <p key={t.id} className="sh-toast">
          {t.text}
        </p>
      ))}
    </div>
  );
}

function Stage() {
  const snapshot = useAtlas((s) => s.snapshot);
  const view = useAtlas((s) => s.view);
  const error = useAtlas((s) => s.error);
  const active = VIEWS.find((v) => v.id === view) ?? VIEWS[0];
  if (!snapshot) return null;
  if (!snapshot.conversations.length) {
    return <EmptyState title="This snapshot has no conversations">The export ran but found nothing to map. Check the export log, then export again.</EmptyState>;
  }
  return (
    <>
      {error ? (
        <p className="sh-banner" role="alert">
          {error}
        </p>
      ) : null}
      {active ? (
        <ViewFrame key={active.id} title={active.title}>
          <active.component />
        </ViewFrame>
      ) : (
        <EmptyState title="No views are registered">Views register themselves from src/views/&lt;group&gt;/index.ts.</EmptyState>
      )}
    </>
  );
}

function Panel() {
  const panel = useUi((s) => s.panel);
  const selected = useAtlas((s) => s.selected);
  if (!panel) return null;
  return (
    <aside className="sh-panel" data-panel={panel} aria-label="Side panel">
      {panel === "insights" ? <InsightsPanel /> : panel === "reader" && selected ? <ReaderPanel /> : selected ? <Detail /> : null}
    </aside>
  );
}

type LoadFailure = "missing" | "incomplete" | "server" | "schema" | "offline" | "other";

/** Reads the store's load error (core/data: `GET <path>: <status> …`, schema mismatch, or a fetch failure). */
function classifyLoadError(error: string): LoadFailure {
  const http = /^GET (\S+): (\d{3})\b/.exec(error);
  if (http) return http[2] !== "404" ? "server" : http[1] === "bundles/manifest.json" ? "missing" : "incomplete";
  if (error.includes("schema_version")) return "schema";
  return /failed to fetch|load failed|networkerror/i.test(error) ? "offline" : "other";
}

const LOAD_ADVICE: Record<LoadFailure, ReactNode> = {
  missing: (
    <>
      No snapshot has been exported yet. Run <code>conv-atlas export</code>, or <code>pnpm fixture</code> for synthetic data.
    </>
  ),
  incomplete: (
    <>
      The newest snapshot is missing a file. Run <code>conv-atlas validate</code>, then export again.
    </>
  ),
  server: (
    <>
      The data server returned an error. Its log is in the terminal running <code>conv-atlas serve</code> or <code>pnpm dev</code>.
    </>
  ),
  schema: <>The snapshot and this page were built for different contract versions. Export again, or rebuild the page.</>,
  offline: (
    <>
      The data server is not answering. Start <code>conv-atlas serve</code> or <code>pnpm dev</code>.
    </>
  ),
  other: null,
};

export function App() {
  const snapshot = useAtlas((s) => s.snapshot);
  const error = useAtlas((s) => s.error);
  const panel = useUi((s) => s.panel);
  useEffect(() => startHashSync(), []);
  // an unknown view (stale link, view not built yet) falls back to the first registered one
  useEffect(
    () =>
      atlas.subscribe(
        (s) => s.view,
        (v) => {
          if (VIEWS.length && !VIEWS.some((x) => x.id === v)) atlas.getState().setView(VIEWS[0].id);
        },
        { fireImmediately: true },
      ),
    [],
  );
  usePanelRules();
  useGlobalKeys();

  if (error && !snapshot) {
    return (
      <div className="sh-app" data-state="error">
        <EmptyState title="Could not load the atlas" tone="error">
          <p className="mono">{error}</p>
          <p>
            {LOAD_ADVICE[classifyLoadError(error)]} The page checks again every {WATCH_INTERVAL_MS / 1000} seconds.
          </p>
        </EmptyState>
      </div>
    );
  }

  return (
    <div className="sh-app" data-panel={panel ?? undefined} aria-busy={!snapshot}>
      <TopBar />
      <FilterBar />
      <main className="sh-stage" aria-label="View">
        {snapshot ? <Stage /> : <EmptyState title="Loading the latest snapshot" />}
      </main>
      <Panel />
      <Transport />
      <CommandPalette />
      <KeysOverlay />
      <Toasts />
    </div>
  );
}
