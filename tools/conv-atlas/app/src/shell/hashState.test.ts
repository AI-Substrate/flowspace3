import { describe, expect, it } from "vitest";
import { NO_FILTERS } from "../core/semantics";
import { formatHash, parseHash } from "./hashState";

describe("shareable link hash", () => {
  it("round-trips view, selection, time and every kind of filter", () => {
    const filters = { ...NO_FILTERS, repos: ["acme/a b"], harnesses: ["claude"], communities: [0, 3], kinds: ["subagent" as const], query: "flaky & slow" };
    const hash = formatHash({ view: "river", selected: "g-1", filters, cursorMs: 1786000000000.4 });
    // empty facets are left out of the link; applying a link fills them from NO_FILTERS
    expect(parseHash(hash)).toEqual({ view: "river", sel: "g-1", t: 1786000000000, filters: { repos: ["acme/a b"], harnesses: ["claude"], communities: [0, 3], kinds: ["subagent"], query: "flaky & slow" } });
  });

  it("omits defaults, so the plain state is a plain link", () => {
    expect(formatHash({ view: "timeline", selected: null, filters: NO_FILTERS, cursorMs: null })).toBe("#/timeline");
  });

  it("still opens the view from a link with mangled filters or a bad time", () => {
    expect(parseHash("#/galaxy?f=%7Bnot-json&t=abc")).toEqual({ view: "galaxy" });
  });

  it("drops filter values of the wrong type instead of trusting the link", () => {
    const f = encodeURIComponent(JSON.stringify({ repos: ["ok", 7], communities: ["x", 2], query: 5 }));
    expect(parseHash(`#/x?f=${f}`).filters).toEqual({ repos: ["ok"], communities: [2] });
  });

  it("ignores hashes that are not app links", () => {
    expect(parseHash("#section-2")).toEqual({});
  });
});
