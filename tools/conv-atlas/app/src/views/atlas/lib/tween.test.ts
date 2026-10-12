import { describe, expect, it } from "vitest";
import { buildLayout } from "./galaxyModel";
import { conv, snapshot } from "./testSnapshot";
import { ADDED, MATCHED, matchTween, REMOVED, tweenPresence } from "./tween";

describe("matchTween", () => {
  const a = conv({ guid: "a", xy: [0, 0] });
  const b = conv({ guid: "b", xy: [1, 1] });
  const gone = conv({ guid: "gone", xy: [-1, -1] });
  const unplaced = conv({ guid: "unplaced", xy: null });
  const prev = snapshot([gone, a, b, unplaced]);
  const next = snapshot([conv({ guid: "b", xy: [-1, 1] }), conv({ guid: "new", xy: [1, -1] }), conv({ guid: "a", xy: [0.5, 0] })]);
  const pl = buildLayout(prev);
  const nl = buildLayout(next);
  const u = matchTween(
    pl,
    prev.conversations.map((c) => c.guid),
    nl,
    next.conversations.map((c) => c.guid),
  );

  it("keeps incoming points at their own indices and appends outgoing-only points", () => {
    expect(u.nextCount).toBe(3);
    expect(Array.from(u.status)).toEqual([MATCHED, ADDED, MATCHED, REMOVED]);
  });

  it("moves matched points from their old position to the new one, by guid", () => {
    // b: prev point 2 → next point 0
    expect([u.fromX[0], u.fromY[0]]).toEqual([pl.x[2], pl.y[2]]);
    expect([u.toX[0], u.toY[0]]).toEqual([nl.x[0], nl.y[0]]);
    // a: prev point 1 → next point 2
    expect([u.fromX[2], u.fromY[2]]).toEqual([pl.x[1], pl.y[1]]);
    expect([u.toX[2], u.toY[2]]).toEqual([nl.x[2], nl.y[2]]);
  });

  it("blooms added points in place and fades removed points in place", () => {
    expect([u.fromX[1], u.fromY[1]]).toEqual([u.toX[1], u.toY[1]]);
    expect([u.fromX[3], u.fromY[3]]).toEqual([pl.x[0], pl.y[0]]);
    expect([u.toX[3], u.toY[3]]).toEqual([pl.x[0], pl.y[0]]);
  });

  it("carries presence: matched keep theirs, added start at 0, removed end at 0", () => {
    const prevW = Float32Array.from([0.3, 0.5, 0.7]);
    const nextW = Float32Array.from([0.9, 0.4, 0.6]);
    const { from, to } = tweenPresence(u, prevW, nextW);
    expect(Array.from(from)).toEqual([prevW[2], 0, prevW[1], prevW[0]].map(Math.fround));
    expect(Array.from(to)).toEqual([nextW[0], nextW[1], nextW[2], 0].map(Math.fround));
  });
});
