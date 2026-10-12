import { describe, expect, it } from "vitest";
import { DAY_MS, NO_FILTERS, visibility } from "../../../core/semantics";
import { countAtOrBefore, PlayheadMask, passMask } from "./gate";
import { conv, DAY0, snapshot } from "./testSnapshot";

describe("countAtOrBefore", () => {
  const t = [1, 3, 3, 7];
  it("counts entries at or before t, inclusive of ties", () => {
    expect(countAtOrBefore(t, 0)).toBe(0);
    expect(countAtOrBefore(t, 1)).toBe(1);
    expect(countAtOrBefore(t, 3)).toBe(3);
    expect(countAtOrBefore(t, 6.9)).toBe(3);
    expect(countAtOrBefore(t, 7)).toBe(4);
    expect(countAtOrBefore([], 5)).toBe(0);
  });
});

describe("PlayheadMask", () => {
  // deterministic pseudo-random walk
  let seed = 42;
  const rnd = () => {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff;
    return seed / 0x7fffffff;
  };
  const s = snapshot(
    Array.from({ length: 60 }, (_, i) =>
      conv({ started_ms: DAY0 + Math.floor(rnd() * 20) * DAY_MS + (i % 3) * 1000, repo: i % 4, kind: i % 5 ? "session" : "subagent" }),
    ),
    { repos: [0, 1, 2, 3].map((k) => ({ key: `r${k}`, label: `r${k}` })) },
  );

  it("agrees with core visibility() across forward, backward and filter changes", () => {
    let filters = NO_FILTERS;
    const gate = new PlayheadMask((i) => s.conversations[i].started_ms, s.conversations.length, passMask(s, filters));
    for (let step = 0; step < 200; step++) {
      if (step % 37 === 0) {
        filters = step % 2 ? { ...NO_FILTERS, repos: ["r1", "r3"] } : { ...NO_FILTERS, kinds: ["subagent"] };
        gate.setPass(passMask(s, filters));
      }
      const cursor = DAY0 + (rnd() * 24 - 2) * DAY_MS;
      gate.seek(cursor);
      expect(Array.from(gate.mask)).toEqual(Array.from(visibility(s, filters, cursor)));
      expect(Array.from(gate.exists)).toEqual(Array.from(visibility(s, NO_FILTERS, cursor)));
    }
  });

  it("reports a change only when something starts or un-starts", () => {
    const gate = new PlayheadMask((i) => s.conversations[i].started_ms, s.conversations.length, passMask(s, NO_FILTERS));
    expect(gate.seek(DAY0 - DAY_MS)).toBe(false);
    expect(gate.seek(DAY0 + 30 * DAY_MS)).toBe(true);
    expect(gate.seek(DAY0 + 31 * DAY_MS)).toBe(false);
    expect(gate.seek(DAY0 - DAY_MS)).toBe(true);
  });
});
