import { useEffect, useRef, useState } from "react";
import { EVENTS, langGet, langList, langToggle, useEvent, type Lang } from "../lib/ipc";
import { useSensors } from "./Rings";

const GB = 1 << 30;
const gb = (b: number) => (b / GB).toFixed(1);
const pct = (used: number, total: number) => (total ? Math.round((used / total) * 100) : 0);
const color = (p: number) => (p >= 90 ? "#f87171" : p >= 75 ? "#fbbf24" : "var(--dock-acc, #7aa2ff)");

function Bar({ label, used, total }: { label: string; used: number; total: number }) {
  const p = pct(used, total);
  return (
    <div className="u-item">
      <div className="u-row">
        <span className="u-l">{label}</span>
        <span className="u-v">
          {gb(used)} / {gb(total)} GB
        </span>
        <b>{p}%</b>
      </div>
      <div className="u-bar">
        <i style={{ width: `${p}%`, background: color(p) }} />
      </div>
    </div>
  );
}

/** RAM and VRAM in use. */
export function MemoryUsage() {
  const s = useSensors();
  return (
    <div className="tile usage no-grow" title="Bellek kullanımı">
      <Bar label="RAM" used={s.ram?.used ?? 0} total={s.ram?.total ?? 0} />
      {s.vram ? <Bar label="VRAM" used={s.vram.used} total={s.vram.total} /> : <div className="u-row dim">VRAM okunamadı (NVIDIA gerekir)</div>}
    </div>
  );
}

const rate = (v: number) => (v >= 10 ? v.toFixed(0) : v.toFixed(1));

/** Drive temperature and current read/write speed. Needs HWiNFO shared memory. */
export function Storage() {
  const s = useSensors();
  const disks = (s.disks ?? []).slice(0, 3);
  return (
    <div className="tile usage storage no-grow" title={s.hwinfo ? "SSD sıcaklığı ve okuma/yazma hızı (MB/s)" : s.hwinfo_stale ? "HWiNFO veri göndermiyor (ücretsiz sürüm 12 saatte durur): Ayarlar > Shared Memory Support'u kapatıp aç" : "Bu değerler için HWiNFO > Ayarlar > Shared Memory Support açık olmalı"}>
      {disks.map((d) => (
        <div className="u-row" key={d.name}>
          <span className="u-name">{d.name}</span>
          <span className="u-temp">{d.temp != null ? `${Math.round(d.temp)}°` : "—"}</span>
          <span className="u-rw">
            R <b>{rate(d.read)}</b>
          </span>
          <span className="u-rw">
            W <b>{rate(d.write)}</b>
          </span>
        </div>
      ))}
      {!disks.length && <span className="dim">{s.hwinfo ? "Disk bulunamadı" : s.hwinfo_stale ? "HWiNFO durdu" : "HWiNFO gerekli"}</span>}
    </div>
  );
}

const speed = (b: number) => (b >= 1e6 ? `${(b / 1e6).toFixed(1)} MB/s` : b >= 1e3 ? `${Math.round(b / 1e3)} KB/s` : `${Math.round(b)} B/s`);

const Arrow = ({ dir }: { dir: "up" | "down" }) => (
  <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
    {dir === "down" ? <path d="M12 4v15M6 13l6 6 6-6" /> : <path d="M12 20V5M6 11l6-6 6 6" />}
  </svg>
);

/** Internet download and upload speed. */
export function NetSpeed() {
  const s = useSensors();
  return (
    <div className="tile usage net no-grow" title="İnternet hızı (fiziksel bağdaştırıcılar)">
      <div className="u-row">
        <span className="u-arrow down"><Arrow dir="down" /></span>
        <b>{speed(s.down ?? 0)}</b>
      </div>
      <div className="u-row">
        <span className="u-arrow up"><Arrow dir="up" /></span>
        <b>{speed(s.up ?? 0)}</b>
      </div>
    </div>
  );
}

/** Current keyboard layout of the focused window; follows Alt+Shift (or Win+Space) on its own. Click opens the layout picker. */
export function KeyboardLang() {
  const [l, setL] = useState<Lang | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  const popOpen = useEvent<boolean>(EVENTS.langOpen, false);
  const openAtDown = useRef(false);
  useEffect(() => {
    // Polled often to follow Alt+Shift; only a change re-renders.
    const load = () => void langGet().then((n) => setL((o) => (o && n && o.code === n.code && o.name === n.name ? o : n)));
    load();
    const t = setInterval(load, 300);
    return () => clearInterval(t);
  }, []);
  return (
    <div
      ref={ref}
      className="tile klang"
      title={l ? `${l.name} · tıkla ya da Alt+Shift ile değiştir` : "Klavye dili"}
      onPointerDown={() => { openAtDown.current = popOpen; }}
      onClick={async () => {
        const r = ref.current!.getBoundingClientRect();
        await langToggle(r.left + r.width / 2, openAtDown.current ? 0 : (await langList()).length, openAtDown.current);
      }}
    >
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
        <rect x="2.5" y="6" width="19" height="12" rx="2.5" />
        <path d="M6.5 10h.01M10 10h.01M14 10h.01M17.5 10h.01M7.5 14h9" />
      </svg>
      <span className="big">{l?.code ?? "--"}</span>
    </div>
  );
}
