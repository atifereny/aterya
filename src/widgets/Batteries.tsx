import { useState } from "react";
import { batteryRefresh, EVENTS, useEvent, type DeviceBattery } from "../lib/ipc";

const color = (p: number) => (p > 50 ? "#4ade80" : p >= 20 ? "#fbbf24" : "#f87171");
const base = { width: 15, height: 15, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 1.8, strokeLinecap: "round", strokeLinejoin: "round" } as const;

const GLYPH = {
  mouse: (
    <svg {...base}>
      <rect x="6.5" y="3" width="11" height="18" rx="5.5" />
      <path d="M12 3v6" />
    </svg>
  ),
  headset: (
    <svg {...base}>
      <path d="M4 15v-3a8 8 0 0116 0v3" />
      <rect x="3" y="14" width="4" height="6" rx="1.6" />
      <rect x="17" y="14" width="4" height="6" rx="1.6" />
    </svg>
  ),
  keyboard: (
    <svg {...base}>
      <rect x="2.5" y="6" width="19" height="12" rx="2.5" />
      <path d="M6 10h.01M10 10h.01M14 10h.01M18 10h.01M7 14h10" />
    </svg>
  ),
};

export function Batteries() {
  const devices = useEvent<DeviceBattery[]>(EVENTS.battery, []);
  const [busy, setBusy] = useState(false);
  const refresh = () => {
    if (busy) return;
    setBusy(true);
    batteryRefresh().catch(() => {}).finally(() => setBusy(false));
  };
  return (
    <div className={busy ? "tile batteries busy" : "tile batteries"} onClick={refresh} role="button" title="Tıkla: yenile">
      <span className="bat-label">{busy ? "Yenileniyor…" : "Yenile"}</span>
      {devices.map((d) => {
        const on = d.connected && d.percent !== null;
        return (
          <span key={d.id} className="bat" title={on ? `${d.name} %${d.percent}${d.charging ? " (şarj oluyor)" : ""}${d.asleep ? " (uyuyor, son bilinen)" : ""}` : `${d.name}: bağlı değil`} style={{ opacity: !on ? 0.45 : d.asleep ? 0.6 : 1 }}>
            {GLYPH[d.id]}
            <span style={on ? { color: color(d.percent!) } : undefined}>{on ? `${d.percent}%` : "—"}</span>
            {on && d.charging && (
              <svg width="10" height="12" viewBox="0 0 10 14" fill="#fbbf24" aria-label="Şarj oluyor"><path d="M6 0L0 8h4l-1 6 6-8H5z" /></svg>
            )}
          </span>
        );
      })}
    </div>
  );
}
