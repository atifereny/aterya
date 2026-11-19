export const ICON_DEFAULT = 30;
export const ICON_MIN = 22;
export const ICON_GAP = 6;

/** Icon size: the chosen size (`max`), shrunk when the apps would not fit in `availablePx`; never below `ICON_MIN` (or `max` if smaller). */
export function iconSizeFor(count: number, availablePx: number, max = ICON_DEFAULT, gap = ICON_GAP): number {
  if (count <= 0) return max;
  const fit = Math.floor(availablePx / count) - gap;
  return Math.max(Math.min(ICON_MIN, max), Math.min(max, fit));
}
