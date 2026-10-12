/**
 * Overview: the landing page for a snapshot. Corpus KPIs, growth, the biggest
 * communities, the busiest seats, harness/model mix, and the corpus caveats.
 * Everything except the caveats follows the filters and the playhead.
 */
import * as d3 from "d3";
import { type ReactNode, useMemo } from "react";
import { NO_FILTERS, dayOf, isFiltered } from "../../core/semantics";
import { atlas, useAtlas } from "../../core/store";
import type { Filters } from "../../core/semantics";
import { accent, categorical, chrome, communityColor, css, harnessColor, neutral, withAlpha } from "../../design/palette";
import { Chip, EmptyState, Swatch, ViewHeader } from "../../design/primitives";
import { type Tally, caveats, filterIsActive, overview, throughDay } from "./transforms";
import { stageStyle, useSize, useTheme } from "./ui";

/** Lists show at least this much, then grow with the stage and scroll inside their panel. */
const LIST_MIN_H = 280;
const listStyle = { display: "grid", gap: 2, alignContent: "start", flex: "1 1 0", minHeight: LIST_MIN_H, overflow: "auto" } as const;
const SPARK_H = 72;
/** Models start past the eight fixed harness slots so a model never shares claude's or omp's colour. */
const MODEL_SLOT0 = 8;
const fmt = d3.format(",");

function Panel({ title, note, children }: { title: string; note?: string; children: ReactNode }) {
  return (
    <section
      style={{
        border: "1px solid var(--line)",
        borderRadius: "var(--r-3)",
        background: "var(--panel)",
        padding: "var(--sp-4)",
        minWidth: 0,
        display: "flex",
        flexDirection: "column",
      }}
    >
      <h3
        style={{
          margin: "0 0 var(--sp-3)",
          fontSize: "var(--fs-sm)",
          fontWeight: "var(--fw-medium)",
          display: "flex",
          flexWrap: "wrap",
          columnGap: 8,
          alignItems: "baseline",
        }}
      >
        {/* the title never wraps; a long note drops to its own line instead */}
        <span style={{ whiteSpace: "nowrap" }}>{title}</span>
        {note ? <span style={{ color: "var(--muted)", fontSize: "var(--fs-xs)", fontWeight: "var(--fw-regular)" }}>{note}</span> : null}
      </h3>
      {children}
    </section>
  );
}

function Kpi({ label, value, of, note }: { label: string; value: number; of?: number; note?: string }) {
  return (
    <div style={{ border: "1px solid var(--line)", borderRadius: "var(--r-3)", background: "var(--panel)", padding: "var(--sp-3) var(--sp-4)" }}>
      <div style={{ color: "var(--muted)", fontSize: "var(--fs-xs)" }}>{label}</div>
      <div style={{ fontSize: "var(--fs-xl)", fontWeight: "var(--fw-strong)", lineHeight: "var(--lh-tight)" }}>{fmt(value)}</div>
      <div style={{ color: "var(--muted)", fontSize: "var(--fs-xs)", minHeight: "1.4em" }}>
        {of !== undefined && of !== value ? `of ${fmt(of)}` : ""}
        {note ? `${of !== undefined && of !== value ? " · " : ""}${note}` : ""}
      </div>
    </div>
  );
}

/** A row whose bar length is n / max, drawn behind the text. Clickable when `onClick` is set. */
function BarRow({ share, active, onClick, title, children }: { share: number; active?: boolean; onClick?: () => void; title?: string; children: ReactNode }) {
  const style = {
    position: "relative" as const,
    display: "grid",
    gridTemplateColumns: "auto 1fr auto",
    gap: 8,
    alignItems: "center",
    width: "100%",
    padding: "4px 6px",
    border: 0,
    borderRadius: "var(--r-1)",
    background: `linear-gradient(to right, var(--accent-soft) ${(share * 100).toFixed(1)}%, transparent 0)`,
    color: "inherit",
    font: "inherit",
    textAlign: "left" as const,
    cursor: onClick ? "pointer" : "default",
    outline: active ? "1px solid var(--accent)" : undefined,
  };
  return onClick ? (
    <button type="button" style={style} onClick={onClick} aria-pressed={!!active} title={title}>
      {children}
    </button>
  ) : (
    <div style={style} title={title}>
      {children}
    </div>
  );
}

/** One horizontal stacked bar of a mix, colours per entry. */
function MixBar({ parts }: { parts: { key: string; n: number; color: string }[] }) {
  const total = d3.sum(parts, (p) => p.n) || 1;
  return (
    <div style={{ display: "flex", height: 10, borderRadius: "var(--r-1)", overflow: "hidden", background: "var(--line)" }}>
      {parts.map((p) => (
        <div key={p.key} style={{ width: `${(100 * p.n) / total}%`, background: p.color }} />
      ))}
    </div>
  );
}

export function Overview() {
  const snapshot = useAtlas((s) => s.snapshot);
  const filters = useAtlas((s) => s.filters);
  const setFilters = useAtlas((s) => s.setFilters);
  // re-render per simulated day while playing, on every move while scrubbing
  const tick = useAtlas((s) => (s.snapshot && s.time.playing ? dayOf(s.snapshot, s.time.cursorMs) : s.time.cursorMs));
  const theme = useTheme();
  const [sparkRef, sparkSize] = useSize<HTMLDivElement>();

  const cursorMs = atlas.getState().time.cursorMs;
  const ov = useMemo(
    () => (snapshot ? overview(snapshot, filters, atlas.getState().time.cursorMs) : null),
    // `tick` is the cursor's render key: recompute as the playhead moves
    [snapshot, filters, tick],
  );
  const notes = useMemo(() => (snapshot ? caveats(snapshot.meta.stats) : []), [snapshot]);

  const spark = useMemo(() => {
    if (!ov || !snapshot || sparkSize.width === 0) return null;
    const days = ov.daily.length;
    const x = d3.scaleLinear([0, Math.max(1, days - 1)], [0, sparkSize.width]);
    const y = d3.scaleLinear([0, d3.max(ov.daily) || 1], [SPARK_H - 2, 2]).nice();
    const area = d3
      .area<number>()
      .x((_, d) => x(d))
      .y0(SPARK_H)
      .y1((v) => y(v))
      .curve(d3.curveMonotoneX);
    const cut = Math.min(days - 1, dayOf(snapshot, Math.min(cursorMs, snapshot.meta.end_ms)));
    return { path: area(Array.from(ov.daily)) ?? "", playX: x(Math.max(0, cut)) };
  }, [ov, snapshot, sparkSize.width, cursorMs]);

  if (!snapshot || !ov) return <div style={stageStyle} />;
  const { meta, conversations, slots } = snapshot;
  const stats = meta.stats;
  const through = throughDay(snapshot, cursorMs);
  const filtered = isFiltered(filters);
  const toggle = (patch: Partial<Filters>) =>
    filterIsActive(filters, patch)
      ? setFilters(Object.fromEntries(Object.keys(patch).map((k) => [k, NO_FILTERS[k as keyof Filters]])))
      : setFilters(patch);

  const ink = css(accent(theme));
  const later = css(withAlpha(accent(theme), 0.22));
  // sticky slot per model key, so a model keeps its colour across filters and snapshot swaps
  const modelColor = (t: Tally) => {
    const slot = t.i === null ? undefined : slots.model.get(meta.models[t.i].key);
    return slot === undefined ? neutral(0.45, theme) : categorical(MODEL_SLOT0 + slot, theme);
  };
  const commMax = ov.communities[0]?.n || 1;
  const seatMax = d3.max(ov.seatMessages, (r) => r.sent + r.received) || 1;
  const harnessName = (t: Tally) => (t.i === null ? null : meta.harnesses[t.i]);
  const startDate = d3.utcFormat("%b %-d, %Y");
  const warnTint = css(withAlpha(chrome("warn", theme), 0.14));

  // Name the real cause: conversations that match but start after the playhead, or none that match at all.
  const emptyState = () => {
    const upcoming = overview(snapshot, filters, meta.end_ms).conversations;
    const clear = filtered ? (
      <button type="button" className="ca-btn" onClick={() => setFilters(NO_FILTERS)}>
        Clear filters
      </button>
    ) : undefined;
    if (upcoming > 0) {
      return (
        <EmptyState title={filtered ? "No matching conversation has started by the playhead" : "No conversations have started by the playhead"}>
          {`${fmt(upcoming)} ${upcoming === 1 ? "conversation starts" : "conversations start"} later. Move the playhead forward to see ${upcoming === 1 ? "it" : "them"}.`}
        </EmptyState>
      );
    }
    const query = filters.query.trim();
    return (
      <EmptyState title="No conversations match these filters" action={clear}>
        {query ? `Nothing in the snapshot matches the search “${query}”. Clear or shorten it.` : "Nothing in the snapshot matches. Widen the filters."}
      </EmptyState>
    );
  };

  return (
    <div style={stageStyle}>
      <ViewHeader
        title="Overview"
        summary={`${snapshot.id} · ${through}${filtered ? " · filtered" : ""}`}
      />
      <div style={{ flex: 1, overflow: "auto", padding: "var(--sp-5)", display: "flex", flexDirection: "column", gap: "var(--sp-5)" }}>
        {ov.conversations === 0 ? emptyState() : null}
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(150px, 1fr))", gap: "var(--sp-3)" }}>
          <Kpi label="Conversations" value={ov.conversations} of={stats.conversations ?? conversations.length} note={`${fmt(ov.subagents)} subagents`} />
          <Kpi label="Turns so far" value={ov.turns} of={stats.turns_total} />
          <Kpi label="Seats" value={ov.seats} of={meta.seats.length} note={`${fmt(ov.seatsWithConversation)} with a matching conversation`} />
          <Kpi label="Repos" value={ov.repos} of={meta.repos.length} />
          <Kpi label="Communities" value={ov.communities.length} of={meta.communities.length} note={ov.unclustered ? `${fmt(ov.unclustered)} unclustered` : undefined} />
        </div>

        <Panel title="Growth" note={`turns per day · ${startDate(new Date(meta.day0_ms))} → ${startDate(new Date(meta.end_ms))}`}>
          <div ref={sparkRef} style={{ height: SPARK_H, position: "relative" }}>
            {spark ? (
              <svg width={sparkSize.width} height={SPARK_H} role="img" aria-label="Turns per day; the part after the playhead is faded">
                <defs>
                  <clipPath id="ov-past">
                    <rect x={0} y={0} width={spark.playX} height={SPARK_H} />
                  </clipPath>
                </defs>
                <path d={spark.path} fill={later} />
                <path d={spark.path} fill={ink} clipPath="url(#ov-past)" />
                <line x1={spark.playX} x2={spark.playX} y1={0} y2={SPARK_H} stroke="var(--ink)" strokeWidth={1} />
              </svg>
            ) : null}
          </div>
        </Panel>

        <div style={{ flex: "1 0 auto", display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(340px, 1fr))", gap: "var(--sp-5)" }}>
          <Panel title="Top communities" note="conversations so far · click to filter">
            <div style={listStyle}>
              {ov.communities.map((t) => {
                const c = meta.communities[t.i as number];
                const patch = { communities: [t.i as number] };
                return (
                  <BarRow key={t.i} share={t.n / commMax} active={filterIsActive(filters, patch)} onClick={() => toggle(patch)} title={c.blurb}>
                    <Swatch color={communityColor(slots.community.get(String(t.i)) ?? null, theme)} />
                    <span style={{ minWidth: 0 }}>
                      <span style={{ fontWeight: "var(--fw-medium)" }}>{c.label}</span>
                      <span style={{ display: "block", color: "var(--muted)", fontSize: "var(--fs-xs)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
                        {c.blurb}
                      </span>
                    </span>
                    <span style={{ fontSize: "var(--fs-xs)" }}>
                      {fmt(t.n)}
                      <span style={{ color: "var(--muted)" }}> / {fmt(c.size)}</span>
                    </span>
                  </BarRow>
                );
              })}
            </div>
          </Panel>

          <Panel title="Top seats" note="peer messages sent · received · click to filter">
            {ov.seatMessages.length ? (
              <div style={listStyle}>
                {ov.seatMessages.map((r) => {
                  const seat = r.i === null ? null : meta.seats[r.i];
                  const patch = seat ? { seats: [seat.id] } : null;
                  return (
                    <BarRow
                      key={r.i ?? "unknown"}
                      share={(r.sent + r.received) / seatMax}
                      active={filterIsActive(filters, patch)}
                      onClick={patch ? () => toggle(patch) : undefined}
                      title={seat ? undefined : "messages whose sender or receiver seat is not known"}
                    >
                      <Swatch color={seat ? harnessColor(seat.harness === null ? null : meta.harnesses[seat.harness], theme) : neutral(0.45, theme)} />
                      <span
                        className={seat ? "mono" : undefined}
                        style={{ minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontSize: "var(--fs-xs)", color: seat ? undefined : "var(--muted)" }}
                      >
                        {seat ? seat.id : "unknown seat"}
                      </span>
                      <span style={{ fontSize: "var(--fs-xs)" }}>
                        {fmt(r.sent)} <span style={{ color: "var(--muted)" }}>· {fmt(r.received)}</span>
                      </span>
                    </BarRow>
                  );
                })}
              </div>
            ) : (
              <div style={{ color: "var(--muted)", fontSize: "var(--fs-sm)" }}>No peer messages by the playhead.</div>
            )}
          </Panel>

          <Panel title="Mix" note="conversations so far">
            <div style={{ display: "grid", gap: "var(--sp-3)" }}>
              <div style={{ color: "var(--muted)", fontSize: "var(--fs-xs)" }}>Harness · click to filter</div>
              <MixBar parts={ov.harnesses.map((t) => ({ key: String(t.i), n: t.n, color: css(harnessColor(harnessName(t), theme)) }))} />
              <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
                {ov.harnesses.map((t) => {
                  const name = harnessName(t);
                  const patch = name === null ? null : { harnesses: [name] };
                  return (
                    <Chip
                      key={String(t.i)}
                      color={harnessColor(name, theme)}
                      count={t.n}
                      selected={filterIsActive(filters, patch)}
                      onClick={patch ? () => toggle(patch) : undefined}
                    >
                      {name ?? "unknown"}
                    </Chip>
                  );
                })}
              </div>
              <div style={{ color: "var(--muted)", fontSize: "var(--fs-xs)", marginTop: "var(--sp-2)" }}>Model · a conversation can use several · {"<synthetic>"} excluded</div>
              <MixBar parts={ov.models.map((t) => ({ key: String(t.i), n: t.n, color: css(modelColor(t)) }))} />
              <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
                {ov.models.map((t) => (
                  <Chip key={String(t.i)} color={modelColor(t)} count={t.n}>
                    {t.i === null ? "unknown" : meta.models[t.i].label}
                  </Chip>
                ))}
              </div>
            </div>
          </Panel>

          <Panel title="Caveats" note="whole corpus · not filtered">
            <div style={{ display: "grid", gap: "var(--sp-3)" }}>
              {notes.map((c) => (
                <div
                  key={c.title}
                  style={{
                    background: c.warn ? warnTint : "var(--raised)",
                    borderRadius: "var(--r-2)",
                    padding: "var(--sp-2) var(--sp-3)",
                  }}
                >
                  <div style={{ fontWeight: "var(--fw-medium)", fontSize: "var(--fs-sm)" }}>
                    {c.warn ? (
                      <span aria-hidden style={{ color: "var(--warn)", marginRight: 6 }}>
                        ⚠
                      </span>
                    ) : null}
                    {c.title}
                  </div>
                  <div style={{ color: c.text ? "var(--ink-2)" : "var(--muted)", fontSize: "var(--fs-sm)", lineHeight: "var(--lh-prose)" }}>
                    {c.text ?? "Not recorded in this snapshot's stats."}
                  </div>
                </div>
              ))}
            </div>
          </Panel>
        </div>
      </div>
    </div>
  );
}
