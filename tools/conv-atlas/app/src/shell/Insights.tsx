/**
 * Insights: findings agents wrote back with `conv-atlas insight add`.
 * Opening one highlights its conversations (`pick`), narrows playback to its
 * time range and shows the write-up. Tour mode plays each insight's range in
 * turn and moves on when playback reaches the end.
 */
import { Fragment, type ReactNode, useEffect, useRef } from "react";
import type { Insight, InsightKind } from "../contract/types";
import { DAY_MS, fullRange } from "../core/semantics";
import { atlas, useAtlas } from "../core/store";
import { kindColor } from "../design/palette";
import { EmptyState, Swatch } from "../design/primitives";
import { fmt } from "./format";
import { useTheme } from "./hooks";
import { Back, Close, Pause, Play } from "./icons";
import { useUi } from "./ui";

const KIND_LABEL: Record<InsightKind, string> = { finding: "Finding", question: "Question", anomaly: "Anomaly", story: "Story" };
const TOUR_HOLD_MS = 6000;
const TOUR_GAP_MS = 1500;
/** a tour step plays its range in about this many seconds, whatever its length */
const TOUR_STEP_S = 8;

/** Narrow playback to the insight and highlight its conversations; a tour also paces the speed. */
function applyInsight(doc: Insight, autoplay: boolean): void {
  const s = atlas.getState();
  const snap = s.snapshot;
  if (!snap) return;
  s.pick((doc.refs.guids ?? []).filter((g) => snap.guidIndex.has(g)));
  const full = fullRange(snap.meta);
  const tr = doc.refs.time_range;
  if (tr) {
    const lo = Math.max(full[0], Math.min(full[1] - DAY_MS, tr.start_ms));
    const hi = Math.min(full[1], Math.max(lo + DAY_MS, tr.end_ms));
    const speed = autoplay ? Math.min(90, Math.max(0.25, (hi - lo) / DAY_MS / TOUR_STEP_S)) : s.time.speed;
    s.setTime({ range: [lo, hi], cursorMs: autoplay ? lo : hi, playing: autoplay, speed });
  } else {
    s.setTime({ range: full, cursorMs: full[1], playing: false });
  }
}

export function clearInsight(): void {
  const s = atlas.getState();
  s.setActiveInsight(null);
  s.pick([]);
  const snap = s.snapshot;
  if (snap) s.setTime({ range: fullRange(snap.meta), playing: false });
  useUi.getState().setTour(null);
}

/** Tiny, safe markdown: paragraphs, "- " lists, "#" headings, `code`, **bold**, http(s) links. No HTML. */
function Markdown({ text }: { text: string }) {
  const inline = (s: string): ReactNode[] =>
    s.split(/(`[^`]+`|\*\*[^*]+\*\*|\[[^\]]+\]\(https?:\/\/[^)\s]+\))/g).map((part, i) => {
      if (part.startsWith("`") && part.endsWith("`") && part.length > 1) return <code key={i}>{part.slice(1, -1)}</code>;
      if (part.startsWith("**") && part.endsWith("**") && part.length > 3) return <strong key={i}>{part.slice(2, -2)}</strong>;
      const link = /^\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)$/.exec(part);
      if (link)
        return (
          <a key={i} href={link[2]} target="_blank" rel="noreferrer">
            {link[1]}
          </a>
        );
      return <Fragment key={i}>{part}</Fragment>;
    });
  return (
    <div className="sh-md">
      {text
        .trim()
        .split(/\n\s*\n/)
        .map((block, i) => {
          const lines = block.split("\n");
          if (lines.every((l) => /^\s*[-*] /.test(l)))
            return (
              <ul key={i}>
                {lines.map((l, j) => (
                  <li key={j}>{inline(l.replace(/^\s*[-*] /, ""))}</li>
                ))}
              </ul>
            );
          const h = /^#{1,6} (.*)$/.exec(block);
          if (h && lines.length === 1) return <h4 key={i}>{inline(h[1])}</h4>;
          return <p key={i}>{inline(lines.join(" "))}</p>;
        })}
    </div>
  );
}

function Refs({ doc }: { doc: Insight }) {
  const snapshot = useAtlas((s) => s.snapshot);
  const openTurn = useAtlas((s) => s.openTurn);
  const select = useAtlas((s) => s.select);
  const r = doc.refs;
  const convs = (r.guids ?? []).map((g) => ({ g, i: snapshot?.guidIndex.get(g) }));
  const missing = convs.filter((c) => c.i === undefined).length;
  return (
    <div className="sh-refs">
      {r.time_range ? (
        <p>
          <span className="sh-muted">Time range</span> {fmt.date(r.time_range.start_ms)} to {fmt.date(r.time_range.end_ms)}
        </p>
      ) : null}
      {convs.length ? (
        <>
          <p className="sh-muted">
            {convs.length === 1 ? "One conversation" : `${convs.length} conversations`}
            {missing ? `, ${missing} not in this snapshot` : ""}
          </p>
          <ul className="sh-reflist">
            {convs
              .filter((c) => c.i !== undefined)
              .slice(0, 30)
              .map(({ g, i }) => (
                <li key={g}>
                  <button type="button" className="sh-link" onClick={() => select(g)}>
                    {snapshot?.conversations[i as number].label}
                  </button>
                </li>
              ))}
          </ul>
        </>
      ) : null}
      {r.addresses?.length ? (
        <ul className="sh-reflist">
          {r.addresses.slice(0, 20).map((a) => {
            const m = /^conv:([^#]+)(?:#t(\d+))?$/.exec(a);
            const known = m && snapshot?.guidIndex.has(m[1]);
            return (
              <li key={a}>
                {m && known ? (
                  <button type="button" className="sh-link mono" onClick={() => (m[2] ? openTurn(m[1], Number(m[2])) : select(m[1]))}>
                    {a.slice(0, 14)}…{m[2] ? `#t${m[2]}` : ""}
                  </button>
                ) : (
                  <code>{a}</code>
                )}
              </li>
            );
          })}
        </ul>
      ) : null}
      {r.seats?.length ? (
        <p>
          <span className="sh-muted">Seats</span> <span className="mono">{r.seats.join(", ")}</span>
        </p>
      ) : null}
      {r.repos?.length ? (
        <p>
          <span className="sh-muted">Repos</span> {r.repos.join(", ")}
        </p>
      ) : null}
    </div>
  );
}

export function InsightsPanel() {
  const index = useAtlas((s) => s.insights);
  const active = useAtlas((s) => s.activeInsight);
  const doc = useAtlas((s) => s.insight);
  const setActiveInsight = useAtlas((s) => s.setActiveInsight);
  const setPanel = useUi((s) => s.setPanel);
  const tour = useUi((s) => s.tour);
  const setTour = useUi((s) => s.setTour);
  useTheme();
  const list = index?.insights ?? [];

  // apply refs once per loaded document
  const applied = useRef<string | null>(null);
  useEffect(() => {
    if (!doc || doc.id !== active) {
      applied.current = null;
      return;
    }
    if (applied.current === doc.id) return;
    applied.current = doc.id;
    applyInsight(doc, useUi.getState().tour !== null);
  }, [doc, active]);

  // tour: advance when the insight's range has played out
  useEffect(() => {
    if (tour === null) return;
    let timer = 0;
    const next = () => {
      const t = useUi.getState().tour;
      if (t === null) return;
      if (t + 1 >= list.length) {
        setTour(null);
        return;
      }
      setTour(t + 1);
      setActiveInsight(list[t + 1].id);
    };
    const unsub = atlas.subscribe(
      (s) => s.time.playing,
      (playing) => {
        const s = atlas.getState();
        if (!playing && s.insight?.id === s.activeInsight && s.time.cursorMs >= s.time.range[1]) timer = window.setTimeout(next, TOUR_GAP_MS);
      },
    );
    if (doc && !doc.refs.time_range) timer = window.setTimeout(next, TOUR_HOLD_MS);
    return () => {
      unsub();
      clearTimeout(timer);
    };
  }, [tour, list, doc, setTour, setActiveInsight]);

  const startTour = () => {
    if (!list.length) return;
    setTour(0);
    setActiveInsight(list[0].id);
  };

  const head = (
    <header className="sh-panelhead">
      {active ? (
        <button type="button" className="ca-btn" data-variant="ghost" onClick={clearInsight} title="Back to all insights and clear the highlight">
          <Back />
          All insights
        </button>
      ) : (
        <h2 className="sh-panelhead__title">Insights</h2>
      )}
      <button type="button" className="ca-btn" data-variant="ghost" data-size="icon" aria-label="Close insights" title="Close (Esc)" onClick={() => setPanel(null)}>
        <Close />
      </button>
    </header>
  );

  if (!list.length) {
    return (
      <section className="sh-insights" aria-label="Insights">
        {head}
        <EmptyState title="No insights yet">
          <p>Insights are findings that agents write back about this corpus. They appear here and on the map as soon as they are added.</p>
          <p>
            Add one with <code>conv-atlas insight add finding.json</code>.
          </p>
        </EmptyState>
      </section>
    );
  }

  if (active) {
    const entry = list.find((x) => x.id === active);
    const pos = list.findIndex((x) => x.id === active);
    return (
      <section className="sh-insights" aria-label="Insight">
        {head}
        {tour !== null ? (
          <div className="sh-tourbar" role="group" aria-label="Tour">
            <span>
              Tour: {pos + 1} of {list.length}
            </span>
            <button type="button" className="ca-btn" data-variant="ghost" disabled={pos <= 0} onClick={() => { setTour(pos - 1); setActiveInsight(list[pos - 1].id); }}>
              Previous
            </button>
            <button type="button" className="ca-btn" data-variant="ghost" disabled={pos >= list.length - 1} onClick={() => { setTour(pos + 1); setActiveInsight(list[pos + 1].id); }}>
              Next
            </button>
            <button type="button" className="ca-btn" data-variant="ghost" onClick={() => setTour(null)}>
              End tour
            </button>
          </div>
        ) : null}
        <div className="sh-scroll">
          {entry ? (
            <p className="sh-insight__kind">
              <Swatch color={kindColor(entry.kind)} shape="square" /> {KIND_LABEL[entry.kind]}
            </p>
          ) : null}
          <h2 className="sh-detail__title">{doc?.title ?? entry?.title}</h2>
          {doc ? (
            <>
              <p className="sh-muted">
                {doc.author}, {fmt.date(doc.created_ms)}
                {doc.snapshot_id !== atlas.getState().snapshot?.id ? ` (written against snapshot ${doc.snapshot_id})` : ""}
              </p>
              <Markdown text={doc.body_md} />
              <h3 className="sh-h3">References</h3>
              <Refs doc={doc} />
            </>
          ) : (
            <div className="sh-skeleton" aria-busy="true" aria-label="Loading insight">
              <span style={{ width: "80%" }} />
              <span style={{ width: "92%" }} />
              <span style={{ width: "64%" }} />
            </div>
          )}
        </div>
      </section>
    );
  }

  return (
    <section className="sh-insights" aria-label="Insights">
      {head}
      <div className="sh-actions sh-actions--pad">
        <button type="button" className="ca-btn" data-variant="primary" onClick={startTour} title="Play every insight's time range in turn">
          {tour === null ? <Play /> : <Pause />}
          Play tour
        </button>
      </div>
      <ol className="sh-insightlist">
        {[...list]
          .sort((a, b) => b.created_ms - a.created_ms)
          .map((x) => (
            <li key={x.id}>
              <button type="button" className="sh-insightrow" onClick={() => setActiveInsight(x.id)}>
                <Swatch color={kindColor(x.kind)} shape="square" />
                <span className="sh-insightrow__title">{x.title}</span>
                <span className="sh-muted">
                  {KIND_LABEL[x.kind]}, {fmt.day(x.created_ms)}
                </span>
              </button>
            </li>
          ))}
      </ol>
    </section>
  );
}
