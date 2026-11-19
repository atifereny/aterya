import { claudeRefresh, EVENTS, useEvent, type ClaudeUsage as Usage } from "../lib/ipc";

const LABEL = { expired: "oturum süresi doldu", limited: "sunucu yoğun", unknown: "bilinmiyor" } as const;
const color = (p: number) => (p >= 90 ? "#f87171" : p >= 70 ? "#fbbf24" : "#34d399");

/** "1sa 25dk" / "25dk" until an ISO time, "" when unknown or past. */
export function untilText(iso: string | null, now = Date.now()): string {
  const ms = iso ? Date.parse(iso) - now : NaN;
  if (!(ms > 0)) return "";
  const m = Math.round(ms / 60000);
  return m >= 60 ? `${Math.floor(m / 60)}sa ${m % 60}dk` : `${m}dk`;
}

/** "6 Sal" (day of month and short weekday) of an ISO time. */
export function dayText(iso: string | null): string {
  const d = iso ? new Date(iso) : null;
  return d && !isNaN(+d) ? d.toLocaleDateString("tr-TR", { day: "numeric", weekday: "short" }) : "";
}

export function tokensText(n: number): string {
  return n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${Math.round(n / 1e3)}K` : String(n);
}

function Row({ label, pct, note }: { label: string; pct: number; note: string }) {
  return (
    <>
      <b>{label}</b>
      <span className="cu-bar">
        <i style={{ width: `${Math.min(100, Math.max(0, pct))}%`, background: color(pct) }} />
      </span>
      <span className="dim">{note}</span>
      <b className="cu-pct">{Math.round(pct)}%</b>
    </>
  );
}

export function ClaudeUsage() {
  const u = useEvent<Usage>(EVENTS.claude, { kind: "unknown" });
  if (u.kind === "refreshing")
    return (
      <div className="tile claude">
        <span className="dim">Claude · yenileniyor…</span>
      </div>
    );
  if (u.kind !== "ok")
    return (
      <div className="tile claude">
        <span className="dim">Claude · {LABEL[u.kind]}</span>
        <button className="claude-btn" onClick={() => void claudeRefresh()} title="En küçük modelle bir kez çalıştırıp verileri getirir">
          Bilgileri getir
        </button>
      </div>
    );
  return (
    <div className="tile claude cu" title="En çok kullanılan model: son 7 günün yerel kayıtlarından (girdi + çıktı + önbellek yazma)">
      <Row label="Oturum" pct={u.fiveHour} note={untilText(u.fiveHourResets)} />
      <Row label="Haftalık" pct={u.weekly} note={dayText(u.weeklyResets)} />
      {u.topModel && (
        <>
          <b>{u.topModel.name}</b>
          <span />
          <span className="dim">en çok</span>
          <b className="cu-pct">{tokensText(u.topModel.tokens)}</b>
        </>
      )}
    </div>
  );
}
