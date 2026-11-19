import { useEffect, useState } from "react";
import { openTaskManager, topGet, type Top, type TopRow } from "../lib/ipc";

function useTop(): Top {
  const [t, setT] = useState<Top>({ cpu: [], ram: [] });
  useEffect(() => {
    const load = () => void topGet().then(setT);
    load();
    const id = setInterval(load, 2000);
    return () => clearInterval(id);
  }, []);
  return t;
}

const bytes = (b: number) => (b >= 1 << 30 ? `${(b / (1 << 30)).toFixed(1)} GB` : `${Math.round(b / (1 << 20))} MB`);

function List({ rows, fmt, title }: { rows: TopRow[]; fmt: (v: number) => string; title: string }) {
  return (
    <div className="tile toplist" title={title} onClick={() => void openTaskManager()}>
      {rows.slice(0, 3).map((r, i) => (
        <div className="top-row" key={i}>
          {r.icon ? <img src={r.icon} width={14} height={14} alt="" draggable={false} /> : <i className="top-ph" />}
          <span className="top-name">{r.name}</span>
          <b>{fmt(r.value)}</b>
        </div>
      ))}
      {!rows.length && <span className="dim">ölçülüyor…</span>}
    </div>
  );
}

export const TopCpu = () => <List rows={useTop().cpu} fmt={(v) => `${v.toFixed(v < 10 ? 1 : 0)}%`} title="İşlemci · Görev Yöneticisini aç" />;
export const TopRam = () => <List rows={useTop().ram} fmt={bytes} title="Bellek · Görev Yöneticisini aç" />;
