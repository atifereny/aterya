import { useEffect, useRef, useState } from "react";
import { brightnessGet, brightnessSet, EVENTS, mediaControl, openNotifications, screentimeGet, useEvent, useSettings, type ClaudeUsage as Usage, type Display, type MediaState, type ScreenTime as Report, type Weather as WeatherData } from "../lib/ipc";
import { weatherLabel } from "../lib/weatherCodes";
import { formatClock } from "../lib/clock";
import { useNow } from "./Clock";
import { useSensors } from "./Rings";

/** Compact, column-shaped versions of the widgets for a dock on the left or right edge (about 50 px wide). */

const level = (p: number) => (p >= 90 ? "#f87171" : p >= 75 ? "#fbbf24" : "var(--dock-acc, #7aa2ff)");

export function SideClock() {
  const now = useNow();
  const two = (n: number) => String(n).padStart(2, "0");
  const showDate = useSettings()?.clockDate ?? true;
  return (
    <div className="tile side clock" onClick={() => void openNotifications()} title={formatClock(now).date}>
      <span className="big">{two(now.getHours())}</span>
      <span className="big">{two(now.getMinutes())}</span>
      {showDate && <span className="dim">{now.toLocaleDateString("tr-TR", { day: "numeric", month: "short" })}</span>}
    </div>
  );
}

export function SideMemory() {
  const s = useSensors();
  const pct = (u: number, t: number) => (t ? Math.round((u / t) * 100) : 0);
  const ram = pct(s.ram?.used ?? 0, s.ram?.total ?? 0);
  const vram = s.vram ? pct(s.vram.used, s.vram.total) : null;
  return (
    <div className="tile side no-grow" title="Bellek kullanımı">
      <span className="dim">RAM</span>
      <b style={{ color: level(ram) }}>{ram}%</b>
      {vram !== null && (
        <>
          <span className="dim">VRAM</span>
          <b style={{ color: level(vram) }}>{vram}%</b>
        </>
      )}
    </div>
  );
}

const short = (b: number) => (b >= 1e6 ? `${(b / 1e6).toFixed(1)}M` : b >= 1e3 ? `${Math.round(b / 1e3)}K` : "0");

export function SideNetSpeed() {
  const s = useSensors();
  return (
    <div className="tile side no-grow" title="İnternet hızı (indirme ve yükleme)">
      <span className="dim">↓</span>
      <b>{short(s.down ?? 0)}</b>
      <span className="dim">↑</span>
      <b>{short(s.up ?? 0)}</b>
    </div>
  );
}

export function SideNowPlaying() {
  const m = useEvent<MediaState | null>(EVENTS.media, null);
  if (!m) return null;
  return (
    <div className="tile side np" title={`${m.artist ? `${m.title} · ${m.artist}` : m.title}\nTıkla: ${m.playing ? "duraklat" : "oynat"}`} onClick={() => mediaControl("toggle")}>
      {m.artDataUrl ? <img className="art" src={m.artDataUrl} alt="" /> : <div className="art" />}
      <span className="np-state">{m.playing ? "⏸" : "▶"}</span>
    </div>
  );
}

export function SideWeather() {
  const w = useEvent<WeatherData | null>(EVENTS.weather, null);
  if (!w) return null;
  const { label, icon } = weatherLabel(w.code);
  return (
    <div className="tile side" title={`${w.city} · ${label}`}>
      <span className="side-icon">{icon}</span>
      <b>{Math.round(w.tempC)}°</b>
    </div>
  );
}

export function SideScreenTime() {
  const [r, setR] = useState<Report>({ total: 0, apps: [] });
  useEffect(() => {
    const load = () => void screentimeGet().then(setR);
    load();
    const t = setInterval(load, 30000);
    return () => clearInterval(t);
  }, []);
  const m = Math.floor(r.total / 60);
  const lines = m >= 60 ? [`${Math.floor(m / 60)}sa`, `${m % 60}dk`] : [`${m}dk`, "ekran"];
  const tip = r.apps.map((a) => `${a.name}  ${Math.floor(a.secs / 60)}dk`).join("\n");
  return (
    <div className="tile side" title={`Ekran süresi\n${tip}`}>
      <b>{lines[0]}</b>
      <span className="dim">{lines[1]}</span>
    </div>
  );
}

export function SideClaude() {
  const u = useEvent<Usage>(EVENTS.claude, { kind: "unknown" });
  const ok = u.kind === "ok";
  return (
    <div className="tile side no-grow" title={ok ? `Claude: oturum %${Math.round(u.fiveHour)}, haftalık %${Math.round(u.weekly)}` : "Claude kullanımı okunamadı"}>
      <span className="dim">5sa</span>
      <b style={{ color: ok ? level(u.fiveHour) : undefined }}>{ok ? `${Math.round(u.fiveHour)}%` : "—"}</b>
      <span className="dim">Hf</span>
      <b style={{ color: ok ? level(u.weekly) : undefined }}>{ok ? `${Math.round(u.weekly)}%` : "—"}</b>
    </div>
  );
}

/** First screen's brightness; the wheel moves every screen together (the full sliders live in the horizontal dock). */
export function SideBrightness() {
  const [ds, setDs] = useState<Display[]>([]);
  const timer = useRef(0);
  useEffect(() => {
    const load = () => void brightnessGet().then((n) => setDs((o) => (o.length === n.length && o.every((d, i) => d.index === n[i].index && d.percent === n[i].percent) ? o : n)));
    load();
    const t = setInterval(load, 4000);
    return () => clearInterval(t);
  }, []);
  if (!ds.length) return null;
  const wheel = (e: React.WheelEvent) => {
    const step = e.deltaY < 0 ? 5 : -5;
    const next = ds.map((d) => ({ ...d, percent: Math.min(100, Math.max(0, d.percent + step)) }));
    setDs(next);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => next.forEach((d) => void brightnessSet(d.index, d.percent)), 150);
  };
  return (
    <div className="tile side no-grow" title="Parlaklık (fareyi tekerleğiyle değiştir)" onWheel={wheel}>
      <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden>
        <circle cx="12" cy="12" r="4" />
        <path d="M12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5.3 5.3l1.4 1.4M17.3 17.3l1.4 1.4M5.3 18.7l1.4-1.4M17.3 6.7l1.4-1.4" />
      </svg>
      <b>{ds[0].percent}%</b>
    </div>
  );
}
