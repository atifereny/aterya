import { useEffect, useState } from "react";
import { screentimeGet, type ScreenTime as Report } from "../lib/ipc";

const fmt = (s: number) => {
  const m = Math.floor(s / 60);
  return m >= 60 ? `${Math.floor(m / 60)}sa ${m % 60}dk` : `${m}dk`;
};

export function ScreenTime() {
  const [r, setR] = useState<Report>({ total: 0, apps: [] });
  useEffect(() => {
    const load = () => void screentimeGet().then(setR);
    load();
    const t = setInterval(load, 30000);
    return () => clearInterval(t);
  }, []);
  const tip = r.apps.map((a) => `${a.name}  ${fmt(a.secs)}`).join("\n");
  return (
    <div className="tile screentime" title={tip || "Henüz veri yok"}>
      <span className="big">{fmt(r.total)}</span>
      <span className="dim">ekran süresi</span>
    </div>
  );
}
