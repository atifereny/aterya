import { describe, expect, it } from "vitest";
import { formatClock, msUntilNextMinute } from "./clock";

describe("clock", () => {
  const d = new Date(2026, 10, 16, 9, 5, 42, 250); // Mon 16 Nov 2026
  it("formats time and Turkish date", () => {
    const c = formatClock(d);
    expect(c.time).toBe("09:05");
    expect(c.date).toMatch(/^Pzt 16 Kas/);
  });
  it("ms until next minute boundary", () => {
    expect(msUntilNextMinute(d)).toBe(60000 - 42250);
    expect(msUntilNextMinute(new Date(2026, 0, 1, 0, 0, 0, 0))).toBe(60000);
  });
});

import { stepVolume } from "./clock";
describe("stepVolume", () => {
  it("steps by 2 and clamps", () => {
    expect(stepVolume(50, -100)).toBe(52);
    expect(stepVolume(50, 100)).toBe(48);
    expect(stepVolume(99, -1)).toBe(100);
    expect(stepVolume(1, 1)).toBe(0);
  });
});
