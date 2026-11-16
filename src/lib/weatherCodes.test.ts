import { describe, expect, it } from "vitest";
import { weatherLabel } from "./weatherCodes";

describe("weatherLabel", () => {
  it("maps known codes", () => {
    expect(weatherLabel(0).label).toBe("Açık");
    for (const c of [1, 2, 3]) expect(weatherLabel(c).label).toBe("Parçalı bulutlu");
    for (const c of [61, 62, 63, 64, 65]) expect(weatherLabel(c).label).toBe("Yağmurlu");
  });
  it("falls back for unknown codes", () => {
    expect(weatherLabel(1234).label).toBe("bilinmiyor");
  });
});
