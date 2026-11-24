import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { calState, calToggle, type CalState } from "../lib/ipc";
import { along } from "../lib/axis";
import { DAY, hhmm, startOfDay } from "../cal/dates";

const EMPTY: CalState = { sources: [], events: [] };

/** Calendar events between two times; refreshed when a feed changes and every minute. */
export function useCalendar(from: number, to: number, reload = 0): CalState {
  const [s, setS] = useState<CalState>(EMPTY);
  useEffect(() => {
    let live = true;
    const load = () => void calState(from, to).then((n) => live && setS((o) => (JSON.stringify(o) === JSON.stringify(n) ? o : n)));
    load();
    const t = setInterval(load, 60000);
    const un = listen("cal://changed", load);
    return () => {
      live = false;
      clearInterval(t);
      void un.then((f) => f());
    };
  }, [from, to, reload]);
  return s;
}

/** Today's date and the next event. */
function useNext() {
  const [day, setDay] = useState(() => startOfDay(new Date()).getTime());
  useEffect(() => {
    const t = setInterval(() => setDay(startOfDay(new Date()).getTime()), 60000);
    return () => clearInterval(t);
  }, []);
  const { events } = useCalendar(day, day + 2 * DAY);
  const now = Date.now();
  const next = events.find((e) => (e.allDay ? e.end > now : e.end > now));
  return { today: new Date(day), next };
}

function label(next: ReturnType<typeof useNext>["next"]) {
  if (!next) return "Etkinlik yok";
  const sameDayAsToday = startOfDay(new Date(next.start)).getTime() <= startOfDay(new Date()).getTime();
  const when = next.allDay ? "Tüm gün" : sameDayAsToday ? hhmm(next.start) : `Yarın ${hhmm(next.start)}`;
  return `${when} · ${next.title}`;
}

export function Calendar() {
  const { today, next } = useNext();
  const ref = useRef<HTMLDivElement>(null);
  return (
    <div ref={ref} className="tile calw" title={next ? `${next.title}${next.location ? ` · ${next.location}` : ""}` : "Takvim"} onClick={() => void calToggle(along(ref.current!.getBoundingClientRect()))}>
      <span className="cal-badge">
        <b>{today.getDate()}</b>
        <em>{today.toLocaleDateString("tr-TR", { month: "short" })}</em>
      </span>
      <span className="cal-next">
        <span className="dim">{today.toLocaleDateString("tr-TR", { weekday: "long" })}</span>
        <span className="cal-line" style={next ? { borderLeftColor: next.color } : undefined}>{label(next)}</span>
      </span>
    </div>
  );
}

export function SideCalendar() {
  const { today, next } = useNext();
  const ref = useRef<HTMLDivElement>(null);
  return (
    <div ref={ref} className="tile side calw" title={next ? label(next) : "Takvim"} onClick={() => void calToggle(along(ref.current!.getBoundingClientRect()))}>
      <span className="dim">{today.toLocaleDateString("tr-TR", { month: "short" })}</span>
      <b className="big">{today.getDate()}</b>
      <span className="dim">{today.toLocaleDateString("tr-TR", { weekday: "short" })}</span>
      {next && <i className="cal-dot" style={{ background: next.color }} />}
    </div>
  );
}
