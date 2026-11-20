import { useEffect, useRef, useState } from "react";
import { brightnessGet, brightnessSet, useSettings, type Display } from "../lib/ipc";

const Sun = ({ size = 14 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden>
    <circle cx="12" cy="12" r="4" />
    <path d="M12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5.3 5.3l1.4 1.4M17.3 17.3l1.4 1.4M5.3 18.7l1.4-1.4M17.3 6.7l1.4-1.4" />
  </svg>
);

/** One slider per display, numbered left to right like they sit on the desk. Two layouts: side by side ("row") or stacked ("column"). */
export function Brightness() {
  const layout = useSettings()?.brightnessLayout ?? "row";
  const [ds, setDs] = useState<Display[] | null>(null);
  const timers = useRef(new Map<number, ReturnType<typeof setTimeout>>());
  const busy = useRef(false); // a read is running, or the user is dragging: do not overwrite their value
  useEffect(() => {
    // Brightness also changes elsewhere (keys, Windows, monitor buttons): re-read now and then and on hover.
    const load = () => {
      if (busy.current || timers.current.size) return;
      busy.current = true;
      void brightnessGet()
        .then((n) => {
          if (timers.current.size) return;
          setDs((o) => (o && o.length === n.length && o.every((d, i) => d.index === n[i].index && d.percent === n[i].percent) ? o : n));
        })
        .finally(() => (busy.current = false));
    };
    load();
    const t = setInterval(load, 4000);
    window.addEventListener("focus", load);
    const tm = timers.current;
    return () => {
      clearInterval(t);
      window.removeEventListener("focus", load);
      tm.forEach(clearTimeout);
    };
  }, []);
  const change = (index: number, v: number) => {
    setDs((cur) => cur && cur.map((d) => (d.index === index ? { ...d, percent: v } : d)));
    clearTimeout(timers.current.get(index));
    timers.current.set(
      index,
      setTimeout(() => {
        void brightnessSet(index, v).finally(() => timers.current.delete(index));
      }, 150),
    );
  };
  return (
    <div className={`tile brightness no-grow br-${layout}`} title="Her ekranın parlaklığı (soldan sağa)">
      {ds === null || !ds.length ? (
        <span className="dim">{ds === null ? "Parlaklık" : "Ekran parlaklığı okunamadı"}</span>
      ) : (
        ds.map((d, i) => (
          <label className="brs" key={d.index} style={{ "--p": `${d.percent}%` } as React.CSSProperties}>
            <i className="brs-fill" />
            <span className="brs-txt">
              <Sun size={layout === "row" ? 15 : 12} />
              <b>{d.index >= 1000 ? "Dizüstü" : i + 1}</b>
              <em>{d.percent}%</em>
            </span>
            <input type="range" min={0} max={100} value={d.percent} onChange={(e) => change(d.index, Number(e.target.value))} aria-label={`Ekran ${i + 1} parlaklığı`} />
          </label>
        ))
      )}
    </div>
  );
}
