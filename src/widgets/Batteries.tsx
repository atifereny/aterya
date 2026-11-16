import { EVENTS, useEvent, type DeviceBattery } from "../lib/ipc";

const ICON = { mouse: "🖱", headset: "🎧", keyboard: "⌨" };
const color = (p: number) => (p > 50 ? "#4ade80" : p >= 20 ? "#fbbf24" : "#f87171");

export function Batteries() {
  const devices = useEvent<DeviceBattery[]>(EVENTS.battery, []);
  return (
    <div className="tile batteries" style={{ flexDirection: "column", alignItems: "stretch", gap: 3 }}>
      {devices.map((d) => {
        const on = d.connected && d.percent !== null;
        return (
          <div key={d.id} title={on ? `${d.name} ${d.percent}%` : `${d.name}: bağlı değil`} style={{ display: "flex", alignItems: "center", gap: 5, opacity: on ? 1 : 0.45 }}>
            <span style={{ fontSize: 11 }}>{ICON[d.id]}</span>
            <span className="bar" style={{ width: 22, height: 4, borderRadius: 2 }}>
              {on && <i style={{ width: `${d.percent}%`, background: color(d.percent!) }} />}
            </span>
            {on && d.charging && <span style={{ fontSize: 10 }}>⚡</span>}
          </div>
        );
      })}
    </div>
  );
}
