export const ICON_DEFAULT = 30;
export const ICON_MIN = 22;
export const ICON_GAP = 6;

export function iconSizeFor(count: number, availablePx: number): number {
  if (count <= 0) return ICON_DEFAULT;
  const fit = Math.floor(availablePx / count) - ICON_GAP;
  return Math.max(ICON_MIN, Math.min(ICON_DEFAULT, fit));
}
