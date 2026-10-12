import { beforeEach, describe, expect, it } from "vitest";
import { colorSlots, resetColorSlots } from "./colorSlots";

describe("colorSlots", () => {
  beforeEach(resetColorSlots);

  it("gives the largest categories the lowest slots on first sight", () => {
    const s = colorSlots("community", [
      { key: "a", count: 5 },
      { key: "b", count: 50 },
      { key: "c", count: 20 },
    ]);
    expect([s.get("b"), s.get("c"), s.get("a")]).toEqual([0, 1, 2]);
  });

  it("keeps a category's slot when sizes reorder in a later snapshot", () => {
    colorSlots("community", [
      { key: "flaky", count: 17 },
      { key: "merge", count: 18 },
    ]);
    const later = colorSlots("community", [
      { key: "flaky", count: 19 },
      { key: "merge", count: 18 },
    ]);
    expect(later.get("merge")).toBe(0);
    expect(later.get("flaky")).toBe(1);
  });

  it("puts a new category in the lowest free slot, never one already taken", () => {
    colorSlots("repo", [
      { key: "x", count: 9 },
      { key: "y", count: 8 },
      { key: "z", count: 7 },
    ]);
    const later = colorSlots("repo", [
      { key: "x", count: 9 },
      { key: "z", count: 7 },
      { key: "new", count: 100 },
    ]);
    expect(later.get("new")).toBe(3);
    expect(later.get("z")).toBe(2);
  });

  it("keeps dimensions independent", () => {
    colorSlots("repo", [{ key: "k", count: 1 }]);
    expect(colorSlots("harness", [{ key: "k", count: 1 }]).get("k")).toBe(0);
  });
});
