export const DAY = 86_400_000;

export const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate());
export const addDays = (d: Date, n: number) => new Date(d.getFullYear(), d.getMonth(), d.getDate() + n);
export const sameDay = (a: Date, b: Date) => a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();

/** The 42 days (six weeks, Monday first) a month page shows. */
export function monthGrid(year: number, month: number): Date[] {
  const first = new Date(year, month, 1);
  const lead = (first.getDay() + 6) % 7; // Monday = 0
  return Array.from({ length: 42 }, (_, i) => new Date(year, month, 1 - lead + i));
}

const two = (n: number) => String(n).padStart(2, "0");
export const hhmm = (ms: number) => {
  const d = new Date(ms);
  return `${two(d.getHours())}:${two(d.getMinutes())}`;
};

/** Does the event touch the day starting at `day`? An all-day event ends at midnight, so its end is exclusive. */
export function onDay(e: { start: number; end: number }, day: Date): boolean {
  const a = startOfDay(day).getTime();
  const b = a + DAY;
  return e.start < b && Math.max(e.end, e.start + 1) > a;
}

export const monthName = (d: Date) => d.toLocaleDateString("tr-TR", { month: "long", year: "numeric" });
export const dayTitle = (d: Date) => d.toLocaleDateString("tr-TR", { day: "numeric", month: "long", weekday: "long" });
export const WEEKDAYS = ["Pzt", "Sal", "Çar", "Per", "Cum", "Cmt", "Paz"];
