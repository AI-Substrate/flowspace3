/**
 * Reader: one conversation turn by turn (virtualised). Shift markers split the
 * stream where the topic moved; turns after the playhead are drawn as "not yet";
 * when the playhead moves it can follow it. `focusTurn` (core) scrolls to and
 * highlights one turn.
 */
import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Session } from "../contract/types";
import { loadSession } from "../core/data";
import { atlas, useAtlas } from "../core/store";
import { css, roleColor } from "../design/palette";
import { EmptyState } from "../design/primitives";
import { copyText } from "./copy";
import { fmt } from "./format";
import { useTheme } from "./hooks";
import { Back, Close, Copy } from "./icons";
import { useUi } from "./ui";

type Row = { kind: "turn"; t: number } | { kind: "shift"; s: number };

const SPEAKER: Record<string, string> = {
  "human/human": "Human",
  "human/system": "System prompt",
  "human/peer": "Peer",
  "agent/peer": "Peer",
  "agent/system": "Agent",
  "agent/human": "Agent",
};

/** Index of the last turn at or before the cursor, or -1. */
function turnAt(session: Session, cursorMs: number): number {
  let lo = 0;
  let hi = session.turns.length - 1;
  let found = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (session.turns[mid].t_ms <= cursorMs) {
      found = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  return found;
}

export function ReaderPanel() {
  const snapshot = useAtlas((s) => s.snapshot);
  const guid = useAtlas((s) => s.selected);
  const focusTurn = useAtlas((s) => s.focusTurn);
  const select = useAtlas((s) => s.select);
  const setPanel = useUi((s) => s.setPanel);
  useTheme();

  const conv = snapshot && guid ? snapshot.conversations[snapshot.guidIndex.get(guid) ?? -1] : undefined;
  const [state, setState] = useState<{ guid: string; session?: Session; error?: string } | null>(null);

  useEffect(() => {
    if (!snapshot || !conv?.session_detail) return;
    let live = true;
    setState({ guid: conv.guid });
    loadSession(snapshot.id, conv.guid).then(
      (session) => live && setState({ guid: conv.guid, session }),
      (e) => live && setState({ guid: conv.guid, error: e instanceof Error ? e.message : String(e) }),
    );
    return () => {
      live = false;
    };
  }, [snapshot, conv]);

  const head = (
    <header className="sh-panelhead">
      <button type="button" className="ca-btn" data-variant="ghost" onClick={() => setPanel("detail")} title="Back to details">
        <Back />
        Details
      </button>
      <button type="button" className="ca-btn" data-variant="ghost" data-size="icon" aria-label="Close reader" title="Close (Esc)" onClick={() => select(null)}>
        <Close />
      </button>
    </header>
  );

  if (!conv) return null;
  const title = (
    <div className="sh-reader__title">
      <h2>{conv.label}</h2>
      <p className="sh-muted">
        {fmt.count(conv.turns)} turns, {fmt.date(conv.started_ms)}
      </p>
    </div>
  );

  if (!conv.session_detail) {
    return (
      <section className="sh-reader" aria-label="Reader">
        {head}
        {title}
        <EmptyState
          title="No turn-by-turn detail in this snapshot"
          action={
            <button type="button" className="ca-btn" onClick={() => copyText(`flowspace3 get conv:${conv.guid}`, "Copied the fs3 command")}>
              <Copy />
              Copy fs3 command
            </button>
          }
        >
          <p>The export keeps per-turn detail for a subset of conversations. This one has its summary, counts and activity only.</p>
          <p>Fetch the full text with fs3 instead.</p>
        </EmptyState>
      </section>
    );
  }

  const ready = state?.guid === conv.guid ? state : null;
  return (
    <section className="sh-reader" aria-label="Reader">
      {head}
      {title}
      {ready?.error ? (
        <EmptyState title="Could not load this conversation" tone="error">
          <p className="mono">{ready.error}</p>
          <p>The snapshot may have been replaced on disk. Load the newest data and try again.</p>
        </EmptyState>
      ) : ready?.session ? (
        <TurnList session={ready.session} guid={conv.guid} focusN={focusTurn?.guid === conv.guid ? focusTurn.n : null} />
      ) : (
        <div className="sh-skeleton" aria-busy="true" aria-label="Loading turns">
          {Array.from({ length: 8 }, (_, i) => (
            <span key={i} style={{ width: `${60 + ((i * 37) % 35)}%` }} />
          ))}
        </div>
      )}
    </section>
  );
}

function TurnList({ session, guid, focusN }: { session: Session; guid: string; focusN: number | null }) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [follow, setFollow] = useState(true);
  const rows = useMemo<Row[]>(() => {
    const byN = new Map(session.shifts.map((s, i) => [s.n, i]));
    const out: Row[] = [];
    session.turns.forEach((t, i) => {
      const s = byN.get(t.n);
      if (s !== undefined) out.push({ kind: "shift", s });
      out.push({ kind: "turn", t: i });
    });
    return out;
  }, [session]);
  const rowOfTurn = useMemo(() => {
    const m = new Int32Array(session.turns.length);
    rows.forEach((r, i) => {
      if (r.kind === "turn") m[r.t] = i;
    });
    return m;
  }, [rows, session]);

  const v = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: (i) => (rows[i].kind === "shift" ? 44 : 64),
    overscan: 8,
  });

  // "now" in this conversation: re-render only when it crosses a turn
  const [now, setNow] = useState(() => turnAt(session, atlas.getState().time.cursorMs));
  useEffect(() => {
    setNow(turnAt(session, atlas.getState().time.cursorMs));
    return atlas.subscribe(
      (s) => turnAt(session, s.time.cursorMs),
      (i) => setNow(i),
    );
  }, [session]);

  // follow the playhead when it moves (not on mount: a reader opens at the top)
  const prevNow = useRef(now);
  useEffect(() => {
    if (prevNow.current === now) return;
    prevNow.current = now;
    if (follow && now >= 0) v.scrollToIndex(rowOfTurn[now], { align: "center" });
  }, [now, follow, rowOfTurn, v]);

  const focusIdx = focusN === null ? -1 : session.turns.findIndex((t) => t.n === focusN);
  useEffect(() => {
    if (focusIdx >= 0) v.scrollToIndex(rowOfTurn[focusIdx], { align: "center" });
  }, [focusIdx, rowOfTurn, v]);

  return (
    <>
      <div className="sh-reader__bar">
        <label className="sh-check" title="Keep the turn at the playhead in view as it moves">
          <input type="checkbox" checked={follow} onChange={(e) => setFollow(e.target.checked)} />
          Follow playhead
        </label>
        <span className="sh-muted">
          {now < 0 ? "Not started at the playhead" : `${fmt.count(now + 1)} of ${fmt.count(session.turns.length)} turns by the playhead`}
        </span>
      </div>
      <div ref={scrollRef} className="sh-reader__scroll" tabIndex={0} aria-label="Turns">
        <ol className="sh-turns" style={{ height: v.getTotalSize() }}>
          {v.getVirtualItems().map((item) => {
            const row = rows[item.index];
            if (row.kind === "shift") {
              const s = session.shifts[row.s];
              return (
                <li key={item.key} ref={v.measureElement} data-index={item.index} className="sh-shift" style={{ transform: `translateY(${item.start}px)` }}>
                  <span className="sh-shift__label">Topic shift: {s.label}</span>
                  <span className="sh-muted">{s.tags.slice(0, 3).join(", ")}</span>
                </li>
              );
            }
            const t = session.turns[row.t];
            const speaker = SPEAKER[`${t.role}/${t.source}`] ?? t.role;
            return (
              <li
                key={item.key}
                ref={v.measureElement}
                data-index={item.index}
                className="sh-turn"
                data-future={row.t > now || undefined}
                data-now={row.t === now || undefined}
                data-focus={row.t === focusIdx || undefined}
                style={{ transform: `translateY(${item.start}px)` }}
              >
                <div className="sh-turn__meta">
                  <span className="sh-turn__dot" style={{ background: css(roleColor(t.role, t.source)) }} aria-hidden="true" />
                  <span className="sh-turn__who">{speaker}</span>
                  <span className="sh-muted">{fmt.time(t.t_ms)}</span>
                  <button type="button" className="sh-turn__n mono" onClick={() => copyText(`flowspace3 get conv:${guid}#t${t.n}`, `Copied the fs3 command for turn ${t.n}`)} title={`Copy: flowspace3 get conv:${guid}#t${t.n}`}>
                    #t{t.n}
                  </button>
                </div>
                <p className="sh-turn__text">{t.label}</p>
                {t.tags.length ? <p className="sh-turn__tags">{t.tags.join(", ")}</p> : null}
              </li>
            );
          })}
        </ol>
      </div>
    </>
  );
}
