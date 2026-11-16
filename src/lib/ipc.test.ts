import { expect, it } from "vitest";
import { EVENTS } from "./ipc";

it("event names match the backend contract", () => {
  expect(Object.values(EVENTS)).toEqual([
    "apps://changed",
    "tray://changed",
    "media://changed",
    "weather://changed",
    "claude://changed",
    "battery://changed",
    "system://changed",
    "dock://visibility",
  ]);
});
