import { VIEWS } from "../core/registry";
import { useAtlas } from "../core/store";
import { Kbd } from "../design/primitives";
import { getTheme, onThemeChange, setTheme } from "../design/theme";
import { useEffect, useState } from "react";
import { fmt, snapshotIdMs } from "./format";
import { useNow } from "./hooks";
import { Insight, Moon, Search, Sun } from "./icons";
import { useUi } from "./ui";
import { useWatcherHealth } from "./watcher";

function ThemeToggle() {
  const [theme, set] = useState(getTheme);
  useEffect(() => onThemeChange(set), []);
  const next = theme === "dark" ? "light" : "dark";
  return (
    <button type="button" className="ca-btn" data-variant="ghost" data-size="icon" onClick={() => setTheme(next)} title={`Switch to ${next} theme (T)`} aria-label={`Switch to ${next} theme`}>
      {theme === "dark" ? <Sun /> : <Moon />}
    </button>
  );
}

function SnapshotStatus() {
  const snapshot = useAtlas((s) => s.snapshot);
  const pending = useAtlas((s) => s.pending);
  const loading = useAtlas((s) => s.loading);
  const acceptPending = useAtlas((s) => s.acceptPending);
  const health = useWatcherHealth();
  const now = useNow();
  if (!snapshot) return null;
  const created = snapshot.meta.created_ms || snapshotIdMs(snapshot.id) || 0;
  const delta = pending ? pending.counts.conversations - snapshot.conversations.length : 0;
  return (
    <div className="sh-snap">
      {pending ? (
        <button
          type="button"
          className="sh-newdata"
          onClick={() => void acceptPending()}
          disabled={loading}
          title={`Snapshot ${pending.id}, exported ${fmt.dateTime(pending.created_ms)}`}
          aria-label={loading ? "Loading new data" : `Load new data${delta > 0 ? `: +${fmt.count(delta)} conversations` : ""}`}
        >
          <span className="sh-newdata__dot" aria-hidden="true" />
          {loading ? "Loading new data" : "New data"}
          {!loading && delta > 0 ? <span className="sh-newdata__delta">+{fmt.count(delta)} conversations</span> : null}
          {loading ? null : <span className="sh-newdata__act">Load</span>}
        </button>
      ) : null}
      <span className="sh-snap__id" title={`Snapshot ${snapshot.id}${snapshot.meta.source === "synthetic" ? " (synthetic fixture)" : ""}`}>
        <span className="sh-live" data-state={health.state} title={health.title} role="status" aria-label={health.title} />
        <span>Exported {fmt.ago(created, now)}</span>
        {snapshot.meta.source === "synthetic" ? <span className="sh-tag">synthetic</span> : null}
      </span>
    </div>
  );
}

export function TopBar() {
  const view = useAtlas((s) => s.view);
  const setView = useAtlas((s) => s.setView);
  const insightCount = useAtlas((s) => s.insights?.insights.length ?? 0);
  const panel = useUi((s) => s.panel);
  const setPanel = useUi((s) => s.setPanel);
  const setPalette = useUi((s) => s.setPalette);
  const go = useUi((s) => s.go);
  const active = VIEWS.find((v) => v.id === view) ?? VIEWS[0];
  return (
    <header className="sh-top">
      <div className="sh-brand" aria-label="conv-atlas">
        <span className="sh-brand__mark" aria-hidden="true" />
        conv-atlas
      </div>
      <nav className="sh-views" aria-label="Views" data-go={go || undefined}>
        {VIEWS.map((v) => (
          <button
            key={v.id}
            type="button"
            className="sh-views__tab"
            aria-current={v.id === active?.id ? "page" : undefined}
            onClick={() => setView(v.id)}
            title={v.hotkey ? `${v.description} (G then ${v.hotkey.toUpperCase()})` : v.description}
            aria-keyshortcuts={v.hotkey ? `G ${v.hotkey.toUpperCase()}` : undefined}
          >
            {go && v.hotkey ? (
              <kbd className="ca-kbd sh-views__key" aria-hidden="true">
                {v.hotkey.toUpperCase()}
              </kbd>
            ) : null}
            {v.title}
          </button>
        ))}
      </nav>
      <div className="sh-top__right">
        <SnapshotStatus />
        <button type="button" className="ca-btn sh-cmdk" onClick={() => setPalette(true)} aria-label="Search and commands" title="Search and commands (⌘K)">
          <Search />
          <span>Jump to…</span>
          <span className="sh-cmdk__keys">
            <Kbd>⌘</Kbd>
            <Kbd>K</Kbd>
          </span>
        </button>
        <button
          type="button"
          className="ca-btn"
          data-variant="ghost"
          aria-pressed={panel === "insights"}
          onClick={() => setPanel(panel === "insights" ? null : "insights")}
          title="Insights (I)"
        >
          <Insight />
          <span>Insights</span>
          {insightCount ? <span className="sh-count">{insightCount}</span> : null}
        </button>
        <ThemeToggle />
      </div>
    </header>
  );
}
