const time = new Intl.DateTimeFormat("tr-TR", { hour: "2-digit", minute: "2-digit", hour12: false });
const wd = new Intl.DateTimeFormat("tr-TR", { weekday: "short" });
const dm = new Intl.DateTimeFormat("tr-TR", { day: "numeric", month: "short" });

export function formatClock(d: Date): { time: string; date: string } {
  return { time: time.format(d), date: `${wd.format(d)} ${dm.format(d)}` };
}

/** Never 0, so a re-armed timer cannot spin. */
export function msUntilNextMinute(d: Date): number {
  return 60000 - (d.getSeconds() * 1000 + d.getMilliseconds());
}

/** Wheel up (negative deltaY) raises volume by 2, clamped 0-100. */
export function stepVolume(v: number, deltaY: number): number {
  return Math.min(100, Math.max(0, v + (deltaY < 0 ? 2 : -2)));
}
