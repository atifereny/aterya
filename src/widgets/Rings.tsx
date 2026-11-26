import { useEffect, useState, type ReactNode } from "react";
import { batteryGet, sensorsGet, type Battery, type Chip, type Sensors } from "../lib/ipc";
import { HwRefresh } from "./HwRefresh";

/** Progress ring: the arc covers `pct` percent, children sit in the middle. */
function Ring({ pct, color, size = 46, spin, children }: { pct: number; color: string; size?: number; spin?: boolean; children: ReactNode }) {
  const r = 20;
  const c = 2 * Math.PI * r;
  return (
    <div className="ring" style={{ width: size, height: size }}>
      <svg viewBox="0 0 48 48" width={size} height={size}>
        <circle cx="24" cy="24" r={r} fill="none" stroke="currentColor" strokeOpacity="0.16" strokeWidth="4" />
        <circle cx="24" cy="24" r={r} fill="none" stroke={color} strokeWidth="4" strokeLinecap="round" strokeDasharray={`${(c * Math.min(100, Math.max(0, pct))) / 100} ${c}`} transform="rotate(-90 24 24)" style={{ transition: "stroke-dasharray 400ms" }} />
        {spin && (
          <g className="ring-spark">
            <circle cx="24" cy="4" r="2.6" fill="#fff" />
          </g>
        )}
      </svg>
      <div className="ring-mid">{children}</div>
    </div>
  );
}

const hot = (t: number) => (t >= 85 ? "#f87171" : t >= 70 ? "#fbbf24" : "#34d399");

/** Laptop battery as a ring; a spark circles it while charging. */
export function BatteryRing() {
  const [b, setB] = useState<Battery | null | undefined>(undefined);
  useEffect(() => {
    const load = () => void batteryGet().then(setB);
    load();
    const t = setInterval(load, 5000);
    return () => clearInterval(t);
  }, []);
  if (b === undefined) return <div className="tile ringtile" />;
  if (b === null)
    return (
      <div className="tile ringtile" title="Bu bilgisayarda pil yok">
        <span className="dim">Pil yok</span>
      </div>
    );
  const color = b.charging || b.percent > 40 ? "#34d399" : b.percent > 20 ? "#fbbf24" : "#f87171";
  return (
    <div className="tile ringtile no-grow" title={`Pil %${b.percent}${b.charging ? " (şarj oluyor)" : b.plugged ? " (şarjda, dolu)" : ""}`}>
      <Ring pct={b.percent} color={color} spin={b.charging}>
        <span className="ring-num">
          {b.charging && <span className="ring-bolt">⚡</span>}
          {b.percent}
        </span>
      </Ring>
    </div>
  );
}

export function useSensors(): Sensors {
  const [s, setS] = useState<Sensors>({ cpu: {}, gpu: {}, ram: { used: 0, total: 0 }, down: 0, up: 0, disks: [], hwinfo: false });
  useEffect(() => {
    const load = () => void sensorsGet().then(setS);
    load();
    const t = setInterval(load, 2000);
    return () => clearInterval(t);
  }, []);
  return s;
}

const dash = "—";

function ChipRing({ name, chip, hint, hw }: { name: string; chip: Chip; hint?: string; hw?: boolean }) {
  const load = chip.load ?? 0;
  return (
    <div className="tile ringtile chip no-grow hwtile" title={hint}>
      {hw && <HwRefresh />}
      <Ring pct={load} color={chip.temp != null ? hot(chip.temp) : "#7aa2ff"}>
        <span className="ring-num small">{chip.temp != null ? `${Math.round(chip.temp)}°` : dash}</span>
      </Ring>
      <div className="chip-col">
        <span className="dim">{name}</span>
        <b>{chip.load != null ? `${Math.round(load)}%` : dash}</b>
        <span className="chip-w">{chip.power != null ? `${Math.round(chip.power)} W` : dash}</span>
      </div>
    </div>
  );
}

export function CpuRing() {
  const s = useSensors();
  return <ChipRing name="CPU" chip={s.cpu} hw hint={s.hwinfo ? undefined : s.hwinfo_stale ? "HWiNFO veri göndermiyor (ücretsiz sürüm 12 saatte durur): Ayarlar > Shared Memory Support'u kapatıp aç" : "Sıcaklık ve watt için HWiNFO > Ayarlar > Shared Memory Support açık olmalı"} />;
}

export function GpuRing() {
  const s = useSensors();
  return <ChipRing name="GPU" chip={s.gpu} hint={s.gpu.load == null ? "NVIDIA sürücüsü bulunamadı" : undefined} />;
}
