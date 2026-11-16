import { describe, expect, it } from "vitest";
import { iconSizeFor } from "./fit";

describe("iconSizeFor", () => {
  it("keeps 30 when it fits", () => expect(iconSizeFor(5, 1000)).toBe(30));
  it("shrinks toward min", () => {
    const s = iconSizeFor(40, 900);
    expect(s).toBeGreaterThanOrEqual(22);
    expect(s).toBeLessThan(30);
    // brief also wants 40*(s+6)<=900, impossible with min 22 (needs s<=16); min wins
  });
  it("floors at 22", () => expect(iconSizeFor(200, 900)).toBe(22));
});
it("shrinks proportionally when between bounds", () => expect(iconSizeFor(30, 900)).toBe(24));
