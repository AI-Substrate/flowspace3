/**
 * Detail drawer for the selected conversation: identity, composition, tags,
 * activity with playhead, subagent family, and the ways out (reader, fs3).
 */
import { useEffect, useMemo, useRef } from "react";
import type { Conversation } from "../contract/types";
import { DAY_MS } from "../core/semantics";
import { atlas, useAtlas } from "../core/store";
import type { LoadedSnapshot } from "../core/data";
import { categorical, communityColor, css, harnessColor, kindColor, roleColor, turnClassColor } from "../design/palette";
import { Chip, Swatch } from "../design/primitives";
import { copyText } from "./copy";
import { fmt } from "./format";
import { useTheme } from "./hooks";
import { Close, Copy, Reader } from "./icons";
import { useUi } from "./ui";

const TURN_TYPES = [
  { key: "work", label: "Work" },
  { key: "tool_call", label: "Tool call" },
  { key: "tool_result", label: "Tool result" },
  { key: "retrieval", label: "Retrieval" },
] as const;

function Bar({ label, parts }: { label: string; parts: { label: string; n: number; color: string }[] }) {
  const total = parts.reduce((s, p) => s + p.n, 0);
  return (
    <div className="sh-bar">
      <div className="sh-bar__head">
        <span>{label}</span>
        <span className="sh-muted">{fmt.count(total)}</span>
      </div>
      <div className="sh-bar__track" role="img" aria-label={`${label}: ${parts.map((p) => `${p.label} ${p.n}`).join(", ")}`}>
        {total > 0 ? parts.map((p) => (p.n > 0 ? <span key={p.label} style={{ flexGrow: p.n, background: p.color }} title={`${p.label}: ${fmt.count(p.n)}`} /> : null)) : null}
      </div>
      <ul className="sh-bar__legend">
        {parts.map((p) => (
          <li key={p.label} data-zero={p.n === 0 || undefined}>
            <span className="sh-bar__key" style={{ background: p.color }} aria-hidden="true" />
            {p.label} <span className="sh-muted">{fmt.count(p.n)}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** Turns per day across the conversation's own span, with the playhead. */
function Sparkline({ conv, snapshot }: { conv: Conversation; snapshot: LoadedSnapshot }) {
  const headRef = useRef<SVGLineElement>(null);
  const day0 = snapshot.meta.day0_ms;
  const first = conv.activity[0]?.[0] ?? 0;
  const last = conv.activity[conv.activity.length - 1]?.[0] ?? first;
  const span = Math.max(1, last - first + 1);
  const max = conv.activity.reduce((m, [, n]) => Math.max(m, n), 1);
  const W = 320;
  const H = 40;
  const bw = W / span;

  useEffect(() => {
    const place = (cursor: number) => {
      const el = headRef.current;
      if (!el) return;
      const x = ((cursor - (day0 + first * DAY_MS)) / (span * DAY_MS)) * W;
      el.style.display = x < 0 || x > W ? "none" : "";
      el.setAttribute("x1", String(x));
      el.setAttribute("x2", String(x));
    };
    place(atlas.getState().time.cursorMs);
    return atlas.subscribe((s) => s.time.cursorMs, place);
  }, [day0, first, span]);

  return (
    <figure className="sh-spark">
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" role="img" aria-label={`Activity by UTC day from ${fmt.utcDate(day0 + first * DAY_MS)} to ${fmt.utcDate(day0 + last * DAY_MS)}, busiest day ${max} turns`}>
        {conv.activity.map(([d, n]) => {
          const h = Math.max(1.5, Math.sqrt(n / max) * (H - 2));
          return <rect key={d} x={(d - first) * bw} y={H - h} width={Math.max(1, bw - (bw > 3 ? 1 : 0))} height={h} />;
        })}
        <line ref={headRef} className="sh-spark__head" y1={0} y2={H} />
      </svg>
      <figcaption className="sh-spark__axis">
        <span>{fmt.utcDay(day0 + first * DAY_MS)}</span>
        <span>{span === 1 ? "one day" : `${span} days`}, UTC</span>
        <span>{fmt.utcDay(day0 + last * DAY_MS)}</span>
      </figcaption>
    </figure>
  );
}

function ConvLink({ i, snapshot }: { i: number; snapshot: LoadedSnapshot }) {
  const c = snapshot.conversations[i];
  const select = useAtlas((s) => s.select);
  return (
    <button type="button" className="sh-convlink" onClick={() => select(c.guid)}>
      <Swatch color={kindColor(c.kind)} />
      <span className="sh-convlink__label">{c.label}</span>
      <span className="sh-muted">{fmt.utcDay(c.started_ms)}</span>
    </button>
  );
}

export function Detail() {
  const snapshot = useAtlas((s) => s.snapshot);
  const guid = useAtlas((s) => s.selected);
  const select = useAtlas((s) => s.select);
  const setFilters = useAtlas((s) => s.setFilters);
  const setPanel = useUi((s) => s.setPanel);
  useTheme();
  const idx = snapshot && guid ? snapshot.guidIndex.get(guid) : undefined;
  const children = useMemo(() => {
    if (!snapshot || idx === undefined) return [];
    return snapshot.edges.subagent.filter(([p]) => p === idx).map(([, c]) => c);
  }, [snapshot, idx]);
  if (!snapshot || idx === undefined) return null;
  const c = snapshot.conversations[idx];
  const { meta } = snapshot;
  const harness = c.harness === null ? null : meta.harnesses[c.harness];
  const address = `conv:${c.guid}`;

  return (
    <section className="sh-detail" aria-labelledby="sh-detail-title">
      <header className="sh-panelhead">
        <div className="sh-panelhead__kind">
          <Swatch color={kindColor(c.kind)} shape="square" />
          {c.kind === "subagent" ? "Subagent" : "Session"}
        </div>
        <button type="button" className="ca-btn" data-variant="ghost" data-size="icon" aria-label="Close details" title="Close (Esc)" onClick={() => select(null)}>
          <Close />
        </button>
      </header>
      <div className="sh-scroll">
        <h2 id="sh-detail-title" className="sh-detail__title">
          {c.label}
        </h2>
        {c.blurb ? <p className="sh-detail__blurb">{c.blurb}</p> : null}

        <div className="sh-actions">
          <button type="button" className="ca-btn" data-variant={c.session_detail ? "primary" : undefined} onClick={() => setPanel("reader")} title="Read this conversation turn by turn (R)">
            <Reader />
            Open reader
          </button>
          <button type="button" className="ca-btn" onClick={() => copyText(`flowspace3 get ${address}`, "Copied the fs3 command")} title={`Copy: flowspace3 get ${address}`}>
            <Copy />
            Copy fs3 command
          </button>
        </div>

        <dl className="sh-facts">
          <dt>Repo</dt>
          <dd>
            {c.repo === null ? (
              <span className="sh-muted">unknown</span>
            ) : (
              <>
                <Swatch color={categorical(snapshot.slots.repo.get(meta.repos[c.repo].key) ?? 0)} /> {meta.repos[c.repo].label}
              </>
            )}
          </dd>
          <dt>Harness</dt>
          <dd>
            {harness ? (
              <>
                <Swatch color={harnessColor(harness)} /> {harness}
              </>
            ) : (
              <span className="sh-muted">unknown</span>
            )}
          </dd>
          <dt>Seat</dt>
          <dd>{c.seat === null ? <span className="sh-muted">none</span> : <span className="mono">{meta.seats[c.seat].id}</span>}</dd>
          <dt>Models</dt>
          <dd>
            {c.models.length ? (
              c.models.map((m) => (
                <span key={m} className="sh-model">
                  <Swatch color={categorical(8 + (snapshot.slots.model.get(meta.models[m]?.key) ?? 0))} /> {meta.models[m]?.label ?? m}
                </span>
              ))
            ) : (
              <span className="sh-muted">unknown</span>
            )}
          </dd>
          <dt>Community</dt>
          <dd>
            {c.community === null ? (
              <span className="sh-muted">unclustered</span>
            ) : (
              <button type="button" className="sh-link" onClick={() => setFilters({ communities: [c.community as number] })} title="Filter to this community">
                <Swatch color={communityColor(snapshot.slots.community.get(String(c.community)) ?? c.community)} /> {meta.communities[c.community].label}
              </button>
            )}
          </dd>
          <dt>Started</dt>
          <dd>{fmt.utcDateTime(c.started_ms)}</dd>
          <dt>Last turn</dt>
          <dd>
            {fmt.utcDateTime(c.last_ms)} <span className="sh-muted">({fmt.span(c.last_ms - c.started_ms)})</span>
          </dd>
          <dt>Address</dt>
          <dd>
            <button type="button" className="sh-link mono" onClick={() => copyText(address, "Copied the address")} title="Copy the fs3 address">
              {address.slice(0, 18)}…
            </button>
          </dd>
        </dl>

        <h3 className="sh-h3">Composition</h3>
        <Bar label="Turn types" parts={TURN_TYPES.map((t) => ({ label: t.label, n: c.counts[t.key], color: css(turnClassColor(t.key)) }))} />
        <Bar
          label="Speakers"
          parts={[
            { label: "Human", n: c.counts.human, color: css(roleColor("human", "human")) },
            { label: "Agent", n: c.counts.agent, color: css(roleColor("agent", "system")) },
            { label: "Peer", n: c.counts.peer, color: css(roleColor("agent", "peer")) },
          ]}
        />

        <h3 className="sh-h3">Activity</h3>
        <Sparkline conv={c} snapshot={snapshot} />

        {c.tags.length ? (
          <>
            <h3 className="sh-h3">Tags</h3>
            <div className="sh-tags">
              {c.tags.map((t) => (
                <Chip key={t} onClick={() => setFilters({ query: t })} title={`Filter by “${t}”`}>
                  {t}
                </Chip>
              ))}
            </div>
          </>
        ) : null}

        {c.parent !== null || children.length ? (
          <>
            <h3 className="sh-h3">Subagents</h3>
            {c.parent !== null ? (
              <div className="sh-family">
                <p className="sh-muted">Spawned by</p>
                <ConvLink i={c.parent} snapshot={snapshot} />
              </div>
            ) : null}
            {children.length ? (
              <div className="sh-family">
                <p className="sh-muted">Spawned {children.length === 1 ? "one subagent" : `${children.length} subagents`}</p>
                {children.map((i) => (
                  <ConvLink key={i} i={i} snapshot={snapshot} />
                ))}
              </div>
            ) : null}
          </>
        ) : null}
      </div>
    </section>
  );
}
