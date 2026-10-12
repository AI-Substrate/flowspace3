/**
 * Seat network model: pij seats as nodes, peer messages aggregated into
 * directed seat→seat links with per-day counts, so the view can ask "how many
 * messages by day d" (cumulative) and "how many on day d" (firing) cheaply.
 * A message whose sender or receiving seat is unknown runs from or to one extra
 * "unknown seat" node, index `meta.seats.length`, so it stays visible.
 */
import type { LoadedSnapshot } from "../../../core/data";
import { DAY_MS } from "../../../core/semantics";
import { countAtOrBefore } from "./gate";

export interface SeatLink {
  /** seat indices into meta.seats, or the unknown-seat node */
  from: number;
  to: number;
  /** ascending days that carried messages */
  days: Int32Array;
  /** cumulative message count through days[k] */
  cum: Float64Array;
}

export interface SeatNet {
  links: SeatLink[];
  /** node index standing for every unknown sender or receiver (= meta.seats.length) */
  unknown: number;
  /** per node (seats, then the unknown seat): first day it appears; Infinity if never */
  firstDay: Float64Array;
  /** messages with neither end known (unknown→unknown); counted per day, never drawn */
  unattributed: SeatLink;
  /** messages from a seat to itself, all seats together; counted per day, never drawn */
  selfSent: SeatLink;
}

/**
 * A message's recipient is its `to` seat, or failing that the seat of its
 * `to_conv` conversation; its sender is `from`. Either may be the unknown node.
 * Self-messages are tallied, not linked.
 */
export function buildSeatNet(s: LoadedSnapshot): SeatNet {
  const { meta, conversations } = s;
  const unknown = meta.seats.length;
  const firstDay = new Float64Array(unknown + 1).fill(Number.POSITIVE_INFINITY);
  meta.seats.forEach((seat, i) => {
    firstDay[i] = Math.floor((seat.first_ms - meta.day0_ms) / DAY_MS);
  });
  const byPair = new Map<string, Map<number, number>>();
  for (const m of s.edges.messages) {
    const to = m.to ?? conversations[m.to_conv].seat ?? unknown;
    const from = m.from ?? unknown;
    // a seat messaging itself has no link to draw; unknown→unknown is kept as the unattributed tally
    const self = to === from && to !== unknown;
    const key = self ? "self" : `${from}>${to}`;
    let days = byPair.get(key);
    if (!days) byPair.set(key, (days = new Map()));
    days.set(m.day, (days.get(m.day) ?? 0) + m.n);
    if (self) continue;
    firstDay[from] = Math.min(firstDay[from], m.day);
    firstDay[to] = Math.min(firstDay[to], m.day);
  }
  const links: SeatLink[] = [];
  const tally = () => ({ from: unknown, to: unknown, days: new Int32Array(0), cum: new Float64Array(0) });
  let unattributed: SeatLink = tally();
  let selfSent: SeatLink = tally();
  for (const [key, days] of byPair) {
    const [from, to] = key === "self" ? [-1, -1] : key.split(">").map(Number);
    const sorted = [...days].sort((a, b) => a[0] - b[0]);
    let sum = 0;
    const link = {
      from,
      to,
      days: Int32Array.from(sorted, ([d]) => d),
      cum: Float64Array.from(sorted, ([, n]) => (sum += n)),
    };
    if (key === "self") selfSent = link;
    else if (from === to) unattributed = link;
    else links.push(link);
  }
  return { links, unknown, firstDay, unattributed, selfSent };
}

/** Messages on the link through the end of `day`. */
export function messagesThrough(link: SeatLink, day: number): number {
  const k = countAtOrBefore(link.days, day);
  return k === 0 ? 0 : link.cum[k - 1];
}

/** Messages on the link during exactly `day`. */
export function messagesOn(link: SeatLink, day: number): number {
  const k = countAtOrBefore(link.days, day);
  if (k === 0 || link.days[k - 1] !== day) return 0;
  return link.cum[k - 1] - (k > 1 ? link.cum[k - 2] : 0);
}
