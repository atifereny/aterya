import { describe, expect, it } from "vitest";
import { batteryShown, batteryUndecided, btKey } from "./batteryPrefs";

describe("battery choices", () => {
  it("built-in devices show unless turned off", () => {
    expect(batteryShown({}, "mouse")).toBe(true);
    expect(batteryShown({ mouse: false }, "mouse")).toBe(false);
  });
  it("Bluetooth devices show only once turned on", () => {
    expect(batteryShown({}, btKey("Buds"))).toBe(false);
    expect(batteryShown({ "bt:Buds": true }, btKey("Buds"))).toBe(true);
  });
  it("a new Bluetooth device is undecided until the user answers", () => {
    expect(batteryUndecided({}, btKey("Buds"))).toBe(true);
    expect(batteryUndecided({ "bt:Buds": false }, btKey("Buds"))).toBe(false);
    expect(batteryUndecided({}, "mouse")).toBe(false);
  });
});
