/**
 * Playback transport: play/pause, speed, scrubber over an activity-density
 * histogram, range brush, loop, show all. It only writes `setTime`; the clock
 * lives in core/runtime.ts. The playhead and readout update imperatively, so
 * playback never re-renders React per frame.
 */
import { type PointerEvent as ReactPointerEvent, useEffect, useMemo, useRef } from "react";
import { atlas, useAtlas } from "../core/store";
import { DAY_MS, filterPredicate, fullRange, isFiltered } from "../core/semantics";
import type { LoadedSnapshot } from "../core/data";
import { accent, css, neutral } from "../design/palette";
import { Chip, SegmentedControl } from "../design/primitives";
import { onThemeChange } from "../design/theme";
import { fmt } from "./format";
import { Loop, Pause, Play, ToEnd } from "./icons";

export const SPEEDS = [
  { value: "1", label: "1 d/s", title: "One day per second" },
  { value: "7", label: "1 wk/s", title: "One week per second" },
  { value: "30", label: "1 mo/s", title: "One month per second" },
  { value: "90", label: "1 qtr/s", title: "One quarter per second" },
];

/** Per-day turn totals: all conversations, and those passing the filters. */
function density(snapshot: LoadedSnapshot, pass: ((i: number) => boolean) | null) {
  const days = snapshot.meta.days;
  const all = new Float64Array(days);
  const kept = new Float64Array(days);
  snapshot.conversations.forEach((c, i) => {
    const ok = pass ? pass(i) : true;
    for (const [d, n] of c.activity) {
      if (d < 0 || d >= days) continue;
      all[d] += n;
      if (ok) kept[d] += n;
    }
  });
  return { all, kept };
}

function drawHistogram(canvas: HTMLCanvasElement, values: Float64Array, max: number, color: string, under?: { values: Float64Array; color: string }) {
  const dpr = devicePixelRatio || 1;
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  canvas.width = Math.round(w * dpr);
  canvas.height = Math.round(h * dpr);
  const ctx = canvas.getContext("2d");
  if (!ctx || !values.length) return;
  ctx.scale(dpr, dpr);
  const bw = w / values.length;
  // sqrt scale: one busy day must not flatten every quiet one
  const y = (v: number) => (max > 0 ? Math.sqrt(v / max) * (h - 2) : 0);
  const bars = (vals: Float64Array, fill: string) => {
    ctx.fillStyle = fill;
    for (let d = 0; d < vals.length; d++) {
      const bh = y(vals[d]);
      if (bh > 0) ctx.fillRect(d * bw, h - Math.max(1, bh), Math.max(1, bw - (bw > 3 ? 1 : 0)), Math.max(1, bh));
    }
  };
  if (under) bars(under.values, under.color);
  bars(values, color);
}

export function Transport() {
  const snapshot = useAtlas((s) => s.snapshot);
  const filters = useAtlas((s) => s.filters);
  const range = useAtlas((s) => s.time.range);
  const playing = useAtlas((s) => s.time.playing);
  const speed = useAtlas((s) => s.time.speed);
  const loop = useAtlas((s) => s.time.loop);
  const setTime = useAtlas((s) => s.setTime);
  const showAll = useAtlas((s) => s.showAll);

  const pastRef = useRef<HTMLCanvasElement>(null);
  const futureRef = useRef<HTMLCanvasElement>(null);
  const pastClipRef = useRef<HTMLDivElement>(null);
  const headRef = useRef<HTMLDivElement>(null);
  const readoutRef = useRef<HTMLOutputElement>(null);
  const sliderRef = useRef<HTMLDivElement>(null);

  const extent = useMemo<[number, number]>(() => (snapshot ? fullRange(snapshot.meta) : [0, 1]), [snapshot]);
  const filtered = isFiltered(filters);
  const dens = useMemo(
    () => (snapshot ? density(snapshot, filtered ? filterPredicate(snapshot, filters) : null) : null),
    [snapshot, filters, filtered],
  );

  // histogram: redraw on data, filter, size and theme changes
  useEffect(() => {
    if (!dens) return;
    const draw = () => {
      const max = dens.all.reduce((m, v) => Math.max(m, v), 0);
      const faint = css(neutral(0.22));
      const past = filtered ? css(accent()) : css(neutral(0.62));
      const future = filtered ? css(neutral(0.42)) : css(neutral(0.34));
      if (pastRef.current) drawHistogram(pastRef.current, dens.kept, max, past, filtered ? { values: dens.all, color: faint } : undefined);
      if (futureRef.current) drawHistogram(futureRef.current, dens.kept, max, future, filtered ? { values: dens.all, color: css(neutral(0.14)) } : undefined);
    };
    draw();
    const ro = new ResizeObserver(draw);
    if (futureRef.current) ro.observe(futureRef.current);
    const off = onThemeChange(draw);
    return () => {
      ro.disconnect();
      off();
    };
  }, [dens, filtered]);

  // playhead + readout: imperative, every frame while playing
  useEffect(() => {
    const place = (cursor: number) => {
      const f = Math.min(1, Math.max(0, (cursor - extent[0]) / (extent[1] - extent[0])));
      if (headRef.current) headRef.current.style.left = `${f * 100}%`;
      if (pastClipRef.current) pastClipRef.current.style.clipPath = `inset(0 ${(1 - f) * 100}% 0 0)`;
      if (readoutRef.current) readoutRef.current.textContent = fmt.dateTime(cursor);
      if (sliderRef.current) {
        sliderRef.current.setAttribute("aria-valuenow", String(Math.round(cursor)));
        sliderRef.current.setAttribute("aria-valuetext", fmt.dateTime(cursor));
      }
    };
    place(atlas.getState().time.cursorMs);
    return atlas.subscribe((s) => s.time.cursorMs, place);
  }, [extent]);

  const msAt = (clientX: number) => {
    const r = sliderRef.current?.getBoundingClientRect();
    if (!r) return extent[0];
    return extent[0] + Math.min(1, Math.max(0, (clientX - r.left) / r.width)) * (extent[1] - extent[0]);
  };
  const clampToRange = (ms: number) => Math.min(range[1], Math.max(range[0], ms));
  // range edges sit on whole days from day 0, but never past the data's last instant
  const snapDay = (ms: number) => Math.min(extent[1], extent[0] + Math.round((ms - extent[0]) / DAY_MS) * DAY_MS);
  const applyRange = (lo: number, hi: number) =>
    setTime({ range: [lo, hi], cursorMs: Math.min(hi, Math.max(lo, atlas.getState().time.cursorMs)) });
  // where each edge may go: within the data, and at least a day from the other edge
  const edgeBounds = (edge: Edge, [lo, hi]: [number, number]): [number, number] =>
    edge === "start" ? [extent[0], Math.max(extent[0], hi - DAY_MS)] : [Math.min(extent[1], lo + DAY_MS), extent[1]];
  const moveEdge = (edge: Edge, ms: number) => {
    const r = atlas.getState().time.range;
    const [min, max] = edgeBounds(edge, r);
    const v = Math.min(max, Math.max(min, ms));
    if (edge === "start") applyRange(v, r[1]);
    else applyRange(r[0], v);
  };

  const onScrub = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    const el = e.currentTarget;
    el.setPointerCapture(e.pointerId);
    const wasPlaying = atlas.getState().time.playing;
    setTime({ cursorMs: clampToRange(msAt(e.clientX)), playing: false });
    const move = (ev: PointerEvent) => setTime({ cursorMs: clampToRange(msAt(ev.clientX)) });
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      if (wasPlaying) setTime({ playing: true });
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  };

  const onBrush = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    const el = e.currentTarget;
    el.setPointerCapture(e.pointerId);
    const a = msAt(e.clientX);
    const move = (ev: PointerEvent) => {
      const b = msAt(ev.clientX);
      const lo = snapDay(Math.min(a, b));
      applyRange(lo, Math.min(extent[1], Math.max(lo + DAY_MS, snapDay(Math.max(a, b)))));
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  };

  const onGrip = (edge: Edge) => (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.stopPropagation(); // a handle drags its own edge, not a new brush
    const el = e.currentTarget;
    el.setPointerCapture(e.pointerId);
    el.focus();
    const move = (ev: PointerEvent) => moveEdge(edge, snapDay(msAt(ev.clientX)));
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  };

  const onGripKey = (edge: Edge) => (e: React.KeyboardEvent<HTMLDivElement>) => {
    const r = atlas.getState().time.range;
    const at = edge === "start" ? r[0] : r[1];
    const days = DAY_STEPS[e.key];
    if (days !== undefined) {
      e.preventDefault();
      const n = days * (e.shiftKey ? 7 : 1);
      // whole-day grid; an end that sits mid-day (the data's last instant) steps to the grid first
      const i = (at - extent[0]) / DAY_MS;
      moveEdge(edge, extent[0] + (n > 0 ? Math.floor(i) : Math.ceil(i)) * DAY_MS + n * DAY_MS);
    } else if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      const [min, max] = edgeBounds(edge, r);
      moveEdge(edge, e.key === "Home" ? min : max);
    }
  };

  const onSliderKey = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const t = atlas.getState().time;
    const step = DAY_STEPS[e.key];
    if (step !== undefined) {
      e.preventDefault();
      setTime({ cursorMs: clampToRange(t.cursorMs + step * (e.shiftKey ? 7 : 1) * DAY_MS), playing: false });
    } else if (e.key === "Home") {
      e.preventDefault();
      setTime({ cursorMs: range[0], playing: false });
    } else if (e.key === "End") {
      e.preventDefault();
      setTime({ cursorMs: range[1], playing: false });
    }
  };

  const narrowed = range[0] > extent[0] || range[1] < extent[1];
  const resetRange = () => setTime({ range: extent });
  const pct = (ms: number) => `${((ms - extent[0]) / (extent[1] - extent[0])) * 100}%`;
  const [startMin, startMax] = edgeBounds("start", range);
  const [endMin, endMax] = edgeBounds("end", range);
  const ticks = useMemo(() => monthTicks(extent), [extent]);

  if (!snapshot) return <footer className="sh-transport" aria-label="Playback" />;

  return (
    <footer className="sh-transport" aria-label="Playback">
      <button
        type="button"
        className="sh-play"
        aria-label={playing ? "Pause" : "Play"}
        title={playing ? "Pause (Space)" : "Play (Space)"}
        onClick={() => togglePlay()}
      >
        {playing ? <Pause /> : <Play />}
      </button>
      <SegmentedControl label="Playback speed" size="sm" value={String(speed)} options={SPEEDS} onChange={(v) => setTime({ speed: Number(v) })} />
      <div className="sh-scrub">
        <div
          ref={sliderRef}
          className="sh-scrub__track"
          role="slider"
          tabIndex={0}
          aria-label="Time"
          aria-valuemin={range[0]}
          aria-valuemax={range[1]}
          onPointerDown={onScrub}
          onKeyDown={onSliderKey}
        >
          <canvas ref={futureRef} className="sh-scrub__hist" aria-hidden="true" />
          <div ref={pastClipRef} className="sh-scrub__past" aria-hidden="true">
            <canvas ref={pastRef} className="sh-scrub__hist" />
          </div>
          {narrowed ? (
            <>
              <div className="sh-scrub__out" style={{ left: 0, width: pct(range[0]) }} aria-hidden="true" />
              <div className="sh-scrub__out" style={{ left: pct(range[1]), right: 0 }} aria-hidden="true" />
              <div className="sh-scrub__edge" style={{ left: pct(range[0]) }} aria-hidden="true" />
              <div className="sh-scrub__edge" style={{ left: pct(range[1]) }} aria-hidden="true" />
            </>
          ) : null}
          <div ref={headRef} className="sh-scrub__head" aria-hidden="true" />
        </div>
        <div
          className="sh-scrub__axis"
          onPointerDown={onBrush}
          onDoubleClick={resetRange}
          title="Drag to set the playback range; double-click to reset"
        >
          {narrowed ? <div className="sh-scrub__range" style={{ left: pct(range[0]), width: `calc(${pct(range[1])} - ${pct(range[0])})` }} /> : null}
          <div
            className="sh-scrub__grip"
            data-on={narrowed || undefined}
            style={{ left: pct(range[0]) }}
            role="slider"
            tabIndex={0}
            aria-label="Playback range start"
            aria-valuemin={startMin}
            aria-valuemax={startMax}
            aria-valuenow={range[0]}
            aria-valuetext={`from ${fmt.day(range[0])}`}
            title="Range start: drag, or arrow keys move it a day (Shift: a week)"
            onPointerDown={onGrip("start")}
            onKeyDown={onGripKey("start")}
          />
          <div
            className="sh-scrub__grip"
            data-on={narrowed || undefined}
            style={{ left: pct(range[1]) }}
            role="slider"
            tabIndex={0}
            aria-label="Playback range end"
            aria-valuemin={endMin}
            aria-valuemax={endMax}
            aria-valuenow={range[1]}
            aria-valuetext={`through ${fmt.day(range[1] - 1)}`}
            title="Range end: drag, or arrow keys move it a day (Shift: a week)"
            onPointerDown={onGrip("end")}
            onKeyDown={onGripKey("end")}
          />
          {ticks.map((t) => (
            <span key={t.ms} className="sh-scrub__tick" style={{ left: pct(t.ms) }}>
              {t.label}
            </span>
          ))}
        </div>
      </div>
      <output ref={readoutRef} className="sh-readout" aria-live="off" />
      {narrowed ? (
        <span className="sh-rangecue" role="status">
          <Chip selected title="playback range" onRemove={resetRange}>
            {fmt.day(range[0])} – {fmt.day(range[1] - 1)}
          </Chip>
        </span>
      ) : null}
      <button
        type="button"
        className="ca-btn"
        data-variant="ghost"
        data-size="icon"
        aria-pressed={loop}
        aria-label="Loop"
        title="Loop playback (L)"
        onClick={() => setTime({ loop: !loop })}
      >
        <Loop />
      </button>
      <button type="button" className="ca-btn" data-variant="ghost" onClick={showAll} title="Jump to the end and stop: show everything (E)">
        <ToEnd />
        Show all
      </button>
    </footer>
  );
}

type Edge = "start" | "end";

/** Days moved per key on the time and range sliders (Shift multiplies by 7). */
const DAY_STEPS: Record<string, number> = { ArrowLeft: -1, ArrowRight: 1, ArrowDown: -1, ArrowUp: 1, PageDown: -30, PageUp: 30 };

/** Play from the start of the range if parked at its end. */
export function togglePlay(): void {
  const { time, setTime } = atlas.getState();
  if (time.playing) setTime({ playing: false });
  else setTime({ playing: true, cursorMs: time.cursorMs >= time.range[1] ? time.range[0] : time.cursorMs });
}

function monthTicks([a, b]: [number, number]): { ms: number; label: string }[] {
  const out: { ms: number; label: string }[] = [];
  const d = new Date(a);
  d.setDate(1);
  d.setHours(0, 0, 0, 0);
  const months = (b - a) / (30 * DAY_MS);
  const every = months > 18 ? 3 : 1;
  for (d.setMonth(d.getMonth() + 1); d.getTime() < b; d.setMonth(d.getMonth() + every)) {
    out.push({ ms: d.getTime(), label: d.getMonth() === 0 ? String(d.getFullYear()) : fmt.month(d.getTime()) });
  }
  return out;
}
