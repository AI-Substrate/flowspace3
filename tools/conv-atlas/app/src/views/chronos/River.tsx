/**
 * Session topic river: for one `session_detail` conversation, the novelty
 * curve over its messages with labelled shift markers, a role × source strip,
 * and a 2-D turn map (density of every message plus a fading trail behind the
 * playhead) that replays the session as the playhead moves through it.
 * Clicking a turn opens it in the reader via `openTurn`.
 */
import * as d3 from "d3";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Ms, Session, SessionTurn } from "../../contract/types";
import { loadSession } from "../../core/data";
import { DAY_MS, NO_FILTERS, filterPredicate, fullRange } from "../../core/semantics";
import { atlas, useAtlas } from "../../core/store";
import { accent, ageRamp, categorical, css, roleColor, withAlpha } from "../../design/palette";
import { EmptyState, Legend, Tooltip, ViewHeader } from "../../design/primitives";
import { labelLanes, shiftMarkers, turnIndexAt } from "./transforms";
import { stageStyle, useSize, useTheme } from "./ui";

const M = { left: 16, right: 16, bottom: 22 };
const LANE_H = 15;
const STRIP_H = 44;
const ROW_Y = { human: 12, agent: 32 } as const;
/** a whole session plays in this many real seconds */
const SESSION_PLAY_SECONDS = 20;
/** shift labels longer than this are truncated (full text in the tooltip) */
const LABEL_MAX_W = 170;
/** turn map: density cell size (px) and how many messages the playhead trail shows */
const CELL = 4;
const TRAIL = 48;
const fmtInt = d3.format(",");
const fmtDate = d3.utcFormat("%-d %b %H:%M UTC");
const SPEAKERS = [
  { key: "human", label: "human", role: "human", source: "human" },
  { key: "injected", label: "injected prompt", role: "human", source: "system" },
  { key: "peer", label: "peer agent", role: "agent", source: "peer" },
  { key: "agent", label: "agent", role: "agent", source: "system" },
] as const;

export function River() {
  const snapshot = useAtlas((s) => s.snapshot);
  const filters = useAtlas((s) => s.filters);
  const selected = useAtlas((s) => s.selected);
  const focusTurn = useAtlas((s) => s.focusTurn);
  const select = useAtlas((s) => s.select);
  const openTurn = useAtlas((s) => s.openTurn);
  const setTime = useAtlas((s) => s.setTime);
  const setFilters = useAtlas((s) => s.setFilters);
  const theme = useTheme();

  const candidates = useMemo(() => {
    if (!snapshot) return [];
    const pass = filterPredicate(snapshot, filters);
    return snapshot.conversations
      .map((c, i) => ({ c, i }))
      .filter(({ c, i }) => c.session_detail && pass(i))
      .sort((a, b) => b.c.turns - a.c.turns);
  }, [snapshot, filters]);

  const selectedIdx = selected && snapshot ? snapshot.guidIndex.get(selected) : undefined;
  const guid =
    selectedIdx !== undefined && snapshot?.conversations[selectedIdx].session_detail
      ? (selected as string)
      : (candidates[0]?.c.guid ?? null);

  const [session, setSession] = useState<Session | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  // bumped by "Try again"; core evicts failed loads, so a re-run refetches
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    if (!snapshot || !guid) return;
    let live = true;
    setLoadError(null);
    loadSession(snapshot.id, guid).then(
      (s) => live && setSession(s),
      (e) => live && setLoadError(e instanceof Error ? e.message : String(e)),
    );
    return () => {
      live = false;
    };
  }, [snapshot, guid, attempt]);
  const current = session && session.guid === guid ? session : null;
  const turns = current?.turns ?? [];
  const canPlay = turns.length >= 2;
  const markers = useMemo(() => (current ? shiftMarkers(current) : []), [current]);

  // remember the playback window we replaced, so "back to fleet range" restores it
  const savedPlayback = useRef<{ range: [Ms, Ms]; speed: number } | null>(null);
  const playSession = () => {
    if (turns.length < 2) return;
    const t = atlas.getState().time;
    savedPlayback.current ??= { range: t.range, speed: t.speed };
    const start = turns[0].t_ms;
    const end = turns[turns.length - 1].t_ms + 1;
    const spanDays = (end - start) / DAY_MS;
    setTime({ range: [start, end], cursorMs: start, speed: spanDays / SESSION_PLAY_SECONDS, playing: true });
  };
  const restoreFleet = () => {
    const snap = atlas.getState().snapshot;
    const saved = savedPlayback.current;
    savedPlayback.current = null;
    const full: [Ms, Ms] = snap ? fullRange(snap.meta) : [0, 0];
    setTime({ range: saved?.range ?? full, speed: saved?.speed ?? 7, playing: false });
  };

  const [plotRef, plotSize] = useSize<HTMLDivElement>();
  const [mapRef, mapSize] = useSize<HTMLDivElement>();
  const w = Math.max(0, plotSize.width - M.left - M.right);

  const x = useMemo(
    () =>
      d3
        .scaleLinear()
        .domain([0, Math.max(1, turns.length - 1)])
        .range([0, w]),
    [turns.length, w],
  );

  // label font, resolved once webfonts are in so measured widths match what renders
  const [labelFont, setLabelFont] = useState("");
  useEffect(() => {
    let live = true;
    document.fonts.ready.then(() => {
      const rootPx = Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
      if (live) setLabelFont(`${rootPx * 0.6875}px ${getComputedStyle(document.body).fontFamily}`);
    });
    return () => {
      live = false;
    };
  }, []);

  // shift labels: measured, truncated to LABEL_MAX_W, flipped left of their marker
  // near the right edge, then stacked into lanes so no two overlap
  const labelled = useMemo(() => {
    const ctx = labelFont ? document.createElement("canvas").getContext("2d") : null;
    if (ctx) ctx.font = labelFont;
    const measure = (s: string) => (ctx ? ctx.measureText(s).width : s.length * 7);
    const placed = markers.map((m) => {
      let text = m.label;
      if (measure(text) > LABEL_MAX_W) {
        while (text.length > 1 && measure(`${text}…`) > LABEL_MAX_W) text = text.slice(0, -1);
        text = `${text.trimEnd()}…`;
      }
      const width = measure(text) + 6;
      const mx = x(m.idx);
      const flip = mx + width > w && mx - width >= 0;
      return { ...m, text, x: mx, flip, start: flip ? mx - width : mx, width };
    });
    const order = placed.map((_, k) => k).sort((a, b) => placed[a].start - placed[b].start);
    const lanes = labelLanes(
      order.map((k) => ({ x: placed[k].start, width: placed[k].width })),
      8,
    );
    const laneOf: number[] = [];
    order.forEach((k, j) => {
      laneOf[k] = lanes[j];
    });
    return placed.map((p, k) => ({ ...p, lane: laneOf[k] }));
  }, [markers, x, w, labelFont]);
  const nLanes = labelled.reduce((n, m) => Math.max(n, m.lane + 1), 0);
  const top = 8 + nLanes * LANE_H;
  const curveH = Math.max(0, plotSize.height - top - STRIP_H - M.bottom - 8);
  const stripTop = top + curveH + 8;

  const series = current?.novelty.series ?? [];
  const yMax = Math.max(0.5, d3.max(series, (p) => p[1]) ?? 0);
  const y = d3.scaleLinear().domain([0, yMax]).range([curveH, 0]);
  const novArea = d3
    .area<[number, number]>()
    .x((p) => x(p[0]))
    .y0(curveH)
    .y1((p) => y(p[1]))
    .curve(d3.curveMonotoneX);

  // turn strip on canvas: one dot per turn, row = role, colour = role × source
  const stripRef = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const cv = stripRef.current;
    if (!cv || w <= 0) return;
    const dpr = window.devicePixelRatio || 1;
    cv.width = Math.round(w * dpr);
    cv.height = Math.round(STRIP_H * dpr);
    const ctx = cv.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, STRIP_H);
    const r = Math.max(1, Math.min(3.5, w / Math.max(1, turns.length) / 1.6));
    const fill: Record<string, string> = {};
    turns.forEach((t, i) => {
      ctx.fillStyle = fill[`${t.role}:${t.source}`] ??= css(roleColor(t.role, t.source, theme));
      ctx.beginPath();
      ctx.arc(x(i), ROW_Y[t.role], r, 0, Math.PI * 2);
      ctx.fill();
    });
  }, [turns, x, w, theme]);

  // turn map: bounded by the session's own xy extent
  const mapSide = Math.max(0, Math.min(mapSize.width - 16, mapSize.height - 40));
  const mapped = useMemo(() => {
    const pts = turns.map((t, i) => ({ i, xy: t.xy })).filter((p): p is { i: number; xy: [number, number] } => !!p.xy);
    const xs = d3.extent(pts, (p) => p.xy[0]) as [number, number];
    const ys = d3.extent(pts, (p) => p.xy[1]) as [number, number];
    const span = Math.max(xs[1] - xs[0], ys[1] - ys[0]) || 1;
    const pad = 12;
    const k = (mapSide - pad * 2) / span;
    const cx = (xs[0] + xs[1]) / 2;
    const cy = (ys[0] + ys[1]) / 2;
    return pts.map((p) => ({ i: p.i, p: [mapSide / 2 + (p.xy[0] - cx) * k, mapSide / 2 - (p.xy[1] - cy) * k] as [number, number] }));
  }, [turns, mapSide]);

  const mapCanvasRef = useRef<HTMLCanvasElement>(null);
  const playRef = useRef<SVGLineElement>(null);
  const futureRef = useRef<SVGRectElement>(null);
  const nowRef = useRef<HTMLSpanElement>(null);

  // playhead: imperative. The density layer is drawn once per session/size/theme;
  // each playhead step only blits it and draws the last TRAIL messages.
  useEffect(() => {
    const cv = mapCanvasRef.current;
    const dpr = window.devicePixelRatio || 1;
    const px = Math.round(mapSide * dpr);
    if (cv) {
      cv.width = px;
      cv.height = px;
    }
    const ctx = cv?.getContext("2d") ?? null;
    ctx?.setTransform(dpr, 0, 0, dpr, 0, 0);

    // density: messages binned into CELL px squares; colour = mean position in the
    // session (early → late), opacity = log count, so 17k points stay readable
    const layer = document.createElement("canvas");
    layer.width = px;
    layer.height = px;
    const lctx = layer.getContext("2d");
    if (lctx && mapped.length) {
      lctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      const cols = Math.ceil(mapSide / CELL) + 1;
      const bins = new Map<number, { n: number; sum: number }>();
      for (const { i, p } of mapped) {
        const key = Math.floor(p[1] / CELL) * cols + Math.floor(p[0] / CELL);
        const b = bins.get(key);
        if (b) {
          b.n++;
          b.sum += i;
        } else bins.set(key, { n: 1, sum: i });
      }
      let maxN = 1;
      for (const b of bins.values()) maxN = Math.max(maxN, b.n);
      const lastI = Math.max(1, turns.length - 1);
      for (const [key, b] of bins) {
        const alpha = 0.15 + 0.6 * (Math.log1p(b.n) / Math.log1p(maxN));
        lctx.fillStyle = css(withAlpha(ageRamp(b.sum / b.n / lastI, theme), alpha));
        lctx.fillRect((key % cols) * CELL, Math.floor(key / cols) * CELL, CELL - 0.5, CELL - 0.5);
      }
    }

    const headRgba = accent(theme);
    const head = css(headRgba);
    const byIdx = d3.bisector((q: { i: number }) => q.i);
    const endMs = atlas.getState().snapshot?.meta.end_ms ?? Number.POSITIVE_INFINITY;
    let lastIdx = Number.NaN;
    const draw = (idx: number) => {
      if (!ctx) return;
      ctx.clearRect(0, 0, mapSide, mapSide);
      ctx.drawImage(layer, 0, 0, mapSide, mapSide);
      const j = byIdx.right(mapped, idx) - 1;
      if (j < 0) return;
      const from = Math.max(0, j - TRAIL);
      ctx.lineWidth = 1.5;
      ctx.lineCap = "round";
      for (let k = from + 1; k <= j; k++) {
        const fresh = 1 - (j - k) / TRAIL;
        ctx.strokeStyle = css(withAlpha(headRgba, 0.05 + 0.65 * fresh * fresh));
        ctx.beginPath();
        ctx.moveTo(...mapped[k - 1].p);
        ctx.lineTo(...mapped[k].p);
        ctx.stroke();
      }
      for (let k = from; k <= j; k++) {
        const fresh = 1 - (j - k) / TRAIL;
        ctx.fillStyle = css(withAlpha(headRgba, 0.15 + 0.85 * fresh));
        ctx.beginPath();
        ctx.arc(mapped[k].p[0], mapped[k].p[1], 1 + 1.8 * fresh, 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.strokeStyle = head;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.arc(mapped[j].p[0], mapped[j].p[1], 6, 0, Math.PI * 2);
      ctx.stroke();
    };
    return atlas.subscribe(
      (s) => s.time.cursorMs,
      (cursor) => {
        const idx = turnIndexAt(turns, cursor);
        if (nowRef.current) {
          const t = turns[idx];
          const text = !turns.length
            ? ""
            : idx < 0
              ? `playhead (${fmtDate(new Date(Math.min(cursor, endMs)))}) is before this session (starts ${fmtDate(new Date(turns[0].t_ms))})`
              : `playhead on turn ${fmtInt(t.n)}, message ${fmtInt(idx + 1)} of ${fmtInt(turns.length)}${idx === turns.length - 1 ? " (the last)" : ""}`;
          if (nowRef.current.textContent !== text) nowRef.current.textContent = text;
        }
        if (idx === lastIdx) return;
        lastIdx = idx;
        const px = idx < 0 ? 0 : x(idx);
        playRef.current?.setAttribute("x1", String(px));
        playRef.current?.setAttribute("x2", String(px));
        playRef.current?.setAttribute("visibility", idx < 0 ? "hidden" : "visible");
        futureRef.current?.setAttribute("x", String(px));
        futureRef.current?.setAttribute("width", String(Math.max(0, w - px)));
        draw(idx);
      },
      { fireImmediately: true },
    );
  }, [turns, mapped, mapSide, x, w, theme]);

  const [hover, setHover] = useState<{ idx: number; at: { x: number; y: number } } | null>(null);
  const turnAtStrip = (e: React.MouseEvent): number => {
    const box = (e.currentTarget as Element).getBoundingClientRect();
    return Math.max(0, Math.min(turns.length - 1, Math.round(x.invert(e.clientX - box.left))));
  };
  const nearestOnMap = (e: React.MouseEvent<HTMLCanvasElement>): number | null => {
    const box = e.currentTarget.getBoundingClientRect();
    const mx = e.clientX - box.left;
    const my = e.clientY - box.top;
    let best: number | null = null;
    let bestD = 64; // 8px radius
    for (const { i, p } of mapped) {
      const d = (p[0] - mx) ** 2 + (p[1] - my) ** 2;
      if (d < bestD) {
        bestD = d;
        best = i;
      }
    }
    return best;
  };
  const hoverAt = (e: React.MouseEvent, idx: number | null) =>
    setHover(idx === null ? null : { idx, at: { x: e.clientX, y: e.clientY } });

  const focusIdx = focusTurn && focusTurn.guid === guid ? turns.findIndex((t) => t.n === focusTurn.n) : -1;
  const conv = guid && snapshot ? snapshot.conversations[snapshot.guidIndex.get(guid) as number] : null;
  const hovered: SessionTurn | undefined = hover ? turns[hover.idx] : undefined;
  const accentCss = css(accent(theme));

  return (
    <div style={stageStyle}>
      <ViewHeader
        title="River"
        summary={
          conv && current ? (
            <>
              <span title="Turns counts every step of the conversation; messages are the human and agent text turns plotted here (tool calls and tool results are left out).">
                {fmtInt(turns.length)} messages of {fmtInt(conv.turns)} turns
              </span>{" "}
              · {markers.length} topic shifts ·{" "}
              <span ref={nowRef} style={{ color: "var(--accent)" }} />
            </>
          ) : conv ? (
            `${fmtInt(conv.turns)} turns · ${loadError ? "detail failed to load" : "loading detail…"}`
          ) : (
            "no session selected"
          )
        }
      >
        <select
          aria-label="Session"
          value={guid ?? ""}
          onChange={(e) => select(e.target.value)}
          style={{ font: "inherit", background: "var(--panel)", color: "var(--ink)", border: "1px solid var(--line)", borderRadius: 4, maxWidth: 320 }}
        >
          {conv && !candidates.some((c) => c.c.guid === guid) && <option value={conv.guid}>{conv.label} (filtered out)</option>}
          {candidates.map(({ c }) => (
            <option key={c.guid} value={c.guid}>
              {c.label} · {d3.format(",")(c.turns)} turns
            </option>
          ))}
        </select>
        <button type="button" className="ca-btn" data-variant="primary" onClick={playSession} disabled={!canPlay}>
          ▶ Play this session
        </button>
        <button type="button" className="ca-btn" data-variant="ghost" onClick={restoreFleet}>
          Back to fleet range
        </button>
      </ViewHeader>
      {loadError && (
        <EmptyState
          title="This session failed to load"
          tone="error"
          action={
            <button type="button" className="ca-btn" onClick={() => setAttempt((n) => n + 1)}>
              Try again
            </button>
          }
        >
          {loadError}
        </EmptyState>
      )}
      <div style={{ position: "relative", flex: 1, minHeight: 0, display: "flex" }}>
        <div ref={plotRef} style={{ position: "relative", flex: 1, minWidth: 0 }}>
          {current && w > 0 && curveH > 0 && (
            <>
              <canvas ref={stripRef} style={{ position: "absolute", left: M.left, top: stripTop, width: w, height: STRIP_H }} />
              <svg width={plotSize.width} height={plotSize.height} style={{ position: "absolute", inset: 0 }}>
                <g transform={`translate(${M.left},0)`}>
                  <path
                    d={novArea(series) ?? ""}
                    transform={`translate(0,${top})`}
                    fill={css(withAlpha(categorical(0, theme), 0.45))}
                    stroke={css(categorical(0, theme))}
                  />
                  <text x={4} y={top + 12} fill="var(--muted)" fontSize="var(--fs-2xs)">
                    novelty ({current.novelty.window}-turn window)
                  </text>
                  {labelled.map((m) => (
                    <g key={`${m.n}-${m.label}`}>
                      <title>{`turn ${fmtInt(m.n)} · ${m.label} · shift score ${m.score.toFixed(2)}`}</title>
                      <line
                        x1={m.x}
                        x2={m.x}
                        y1={8 + m.lane * LANE_H + 4}
                        y2={stripTop + STRIP_H}
                        stroke="var(--line-strong)"
                        strokeDasharray="2 3"
                      />
                      <text
                        x={m.flip ? m.x - 3 : m.x + 3}
                        y={8 + m.lane * LANE_H + 10}
                        textAnchor={m.flip ? "end" : "start"}
                        fill="var(--ink)"
                        stroke="var(--bg)"
                        strokeWidth={3}
                        paintOrder="stroke"
                        fontSize="var(--fs-xs)"
                      >
                        {m.text}
                      </text>
                    </g>
                  ))}
                  <g transform={`translate(0,${stripTop})`}>
                    <line x1={0} x2={w} y1={ROW_Y.human} y2={ROW_Y.human} stroke="var(--line)" />
                    <line x1={0} x2={w} y1={ROW_Y.agent} y2={ROW_Y.agent} stroke="var(--line)" />
                    {focusIdx >= 0 && (
                      <circle cx={x(focusIdx)} cy={ROW_Y[turns[focusIdx].role]} r={6} fill="none" stroke={accentCss} strokeWidth={2} />
                    )}
                    <rect
                      width={w}
                      height={STRIP_H}
                      fill="transparent"
                      style={{ cursor: "pointer" }}
                      onMouseMove={(e) => hoverAt(e, turnAtStrip(e))}
                      onMouseLeave={() => setHover(null)}
                      onClick={(e) => guid && openTurn(guid, turns[turnAtStrip(e)].n)}
                    />
                  </g>
                  <rect
                    ref={futureRef}
                    y={top}
                    height={stripTop + STRIP_H - top}
                    fill="var(--bg)"
                    fillOpacity={0.6}
                    style={{ pointerEvents: "none" }}
                  />
                  <line ref={playRef} y1={top - 4} y2={stripTop + STRIP_H} stroke={accentCss} strokeWidth={1.5} style={{ pointerEvents: "none" }} />
                  <g transform={`translate(0,${stripTop + STRIP_H + 14})`}>
                    {x.ticks(Math.max(2, Math.floor(w / 100))).map((v) => {
                      const t = turns[Math.round(v)];
                      return t ? (
                        <text key={v} x={x(v)} textAnchor={v === 0 ? "start" : "middle"} fill="var(--muted)" fontSize="var(--fs-2xs)">
                          turn {fmtInt(t.n)}
                        </text>
                      ) : null;
                    })}
                  </g>
                </g>
              </svg>
            </>
          )}
          {!current && !loadError && guid && <EmptyState title="Loading session…" />}
          {snapshot && !guid && (
            <EmptyState
              title="No session detail matches these filters"
              action={
                <button type="button" className="ca-btn" onClick={() => setFilters(NO_FILTERS)}>
                  Clear filters
                </button>
              }
            >
              The river needs a conversation exported with per-turn detail (long sessions).
            </EmptyState>
          )}
        </div>
        <div
          ref={mapRef}
          style={{
            width: "min(380px, 34%)",
            borderLeft: "1px solid var(--line)",
            display: "flex",
            flexDirection: "column",
            gap: 8,
            padding: 8,
          }}
        >
          <span style={{ color: "var(--muted)", fontSize: "var(--fs-xs)" }}>
            Turn map · nearby messages are similar · colour = early → late in the session, stronger = more messages · bright trail = last{" "}
            {TRAIL} messages up to the playhead
          </span>
          {current && mapSide > 0 && (
            <canvas
              ref={mapCanvasRef}
              style={{ width: mapSide, height: mapSide, cursor: "pointer", alignSelf: "center" }}
              onMouseMove={(e) => hoverAt(e, nearestOnMap(e))}
              onMouseLeave={() => setHover(null)}
              onClick={(e) => {
                const i = nearestOnMap(e);
                if (guid && i !== null) openTurn(guid, turns[i].n);
              }}
            />
          )}
          <Legend
            title="Speaker (rows: human / agent)"
            items={SPEAKERS.map((s) => ({ key: s.key, label: s.label, color: roleColor(s.role, s.source, theme) }))}
          />
        </div>
      </div>
      <Tooltip at={hover && hovered ? hover.at : null}>
        {hovered && (
          <>
            <div style={{ color: "var(--muted)" }}>
              <span className="mono">turn {fmtInt(hovered.n)}</span> ·{" "}
              {SPEAKERS.find((s) => s.role === hovered.role && s.source === hovered.source)?.label ?? hovered.role} ·{" "}
              {fmtDate(new Date(hovered.t_ms))}
            </div>
            <div>{hovered.label}</div>
            {hovered.tags.length > 0 && <div style={{ color: "var(--muted)" }}>{hovered.tags.join(" · ")}</div>}
            <div style={{ color: "var(--muted)" }}>click to open in the reader</div>
          </>
        )}
      </Tooltip>
    </div>
  );
}
