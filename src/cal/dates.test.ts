import { describe, expect, it } from "vitest";
import { monthGrid, onDay, sameDay } from "./dates";

describe("monthGrid", () => {
  it("starts on a Monday and holds six weeks", () => {
    const g = monthGrid(2026, 10); // November 2026 starts on a Sunday
    expect(g).toHaveLength(42);
    expect(g[0].getDay()).toBe(1);
    expect(sameDay(g[0], new Date(2026, 9, 26))).toBe(true);
    expect(g.some((d) => sameDay(d, new Date(2026, 10, 30)))).toBe(true);
  });
});

describe("onDay", () => {
  const day = new Date(2026, 10, 24);
  const at = (h: number, d = 24) => new Date(2026, 10, d, h).getTime();
  it("timed events", () => {
    expect(onDay({ start: at(9), end: at(10) }, day)).toBe(true);
    expect(onDay({ start: at(9, 25), end: at(10, 25) }, day)).toBe(false);
  });
  it("an all-day event ends at midnight of the next day", () => {
    const e = { start: new Date(2026, 10, 24).getTime(), end: new Date(2026, 10, 25).getTime() };
    expect(onDay(e, day)).toBe(true);
    expect(onDay(e, new Date(2026, 10, 25))).toBe(false);
  });
});
