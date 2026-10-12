import { describe, expect, it } from "vitest";
import { buildSeatNet, messagesOn, messagesThrough } from "./seatsModel";
import { conv, DAY0, snapshot } from "./testSnapshot";

const DAY = 86_400_000;

describe("buildSeatNet", () => {
  const seats = (["alpha", "beta", "gamma"] as const).map((id, i) => ({
    id,
    harness: null,
    conversations: [],
    first_ms: DAY0 + [2, 6, 3][i] * DAY,
    last_ms: DAY0 + 9 * DAY,
  }));
  const s = snapshot([conv({ seat: 2 }), conv({ seat: null }), conv({ seat: 1 })], { seats });
  s.edges.messages = [
    { from: 0, to: 1, to_conv: 2, day: 2, n: 3 },
    { from: 0, to: 1, to_conv: 2, day: 5, n: 1 },
    { from: 0, to: 1, to_conv: 2, day: 2, n: 2 },
    // recipient resolved through the conversation's seat
    { from: 1, to: null, to_conv: 0, day: 4, n: 4 },
    // receiving conversation has no seat: the message goes to the unknown seat
    { from: 1, to: null, to_conv: 1, day: 4, n: 7 },
    // self message: tallied, never a link
    { from: 2, to: 2, to_conv: 0, day: 1, n: 9 },
    // unknown senders share the same node
    { from: null, to: 0, to_conv: 1, day: 7, n: 2 },
    { from: null, to: null, to_conv: 0, day: 8, n: 1 },
    // neither end known: counted, not drawn
    { from: null, to: null, to_conv: 1, day: 8, n: 5 },
  ];
  const net = buildSeatNet(s);
  const link = (from: number, to: number) => net.links.find((l) => l.from === from && l.to === to);

  it("aggregates messages per direction and resolves recipients via to_conv", () => {
    expect(net.links.map((l) => [l.from, l.to])).toEqual([
      [0, 1],
      [1, 2],
      [1, 3],
      [3, 0],
      [3, 2],
    ]);
  });

  it("routes unknown senders and receivers through one node after the seats", () => {
    expect(net.unknown).toBe(3);
    expect(messagesThrough(link(1, 3) ?? fail(), 9)).toBe(7);
    expect(messagesThrough(link(3, 0) ?? fail(), 9)).toBe(2);
    expect(messagesThrough(link(3, 2) ?? fail(), 9)).toBe(1);
    expect([7, 8].map((d) => messagesThrough(net.unattributed, d))).toEqual([0, 5]);
  });

  it("tallies self-messages apart so the header can still count them", () => {
    expect([0, 1, 9].map((d) => messagesThrough(net.selfSent, d))).toEqual([0, 9, 9]);
  });

  it("answers cumulative and same-day counts at any day", () => {
    const ab = link(0, 1) ?? fail();
    expect([1, 2, 4, 5, 9].map((d) => messagesThrough(ab, d))).toEqual([0, 5, 5, 6, 6]);
    expect([1, 2, 3, 5].map((d) => messagesOn(ab, d))).toEqual([0, 5, 0, 1]);
  });

  it("places each node's first appearance at its seat lifetime start or earliest message", () => {
    // beta's lifetime starts day 6 but it received messages on day 2
    expect(Array.from(net.firstDay)).toEqual([2, 2, 3, 4]);
  });
});

function fail(): never {
  throw new Error("missing link");
}
