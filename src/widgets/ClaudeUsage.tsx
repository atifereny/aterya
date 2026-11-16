import { EVENTS, useEvent, type ClaudeUsage as Usage } from "../lib/ipc";

const Bar = ({ pct, color }: { pct: number; color: string }) => (
  <div style={{ height: 3, borderRadius: 2, background: "rgba(255,255,255,0.15)" }}>
    <div style={{ width: `${Math.min(100, Math.max(0, pct))}%`, height: "100%", borderRadius: 2, background: color }} />
  </div>
);

export function ClaudeUsage() {
  const u = useEvent<Usage>(EVENTS.claude, { kind: "unknown" });
  if (u.kind !== "ok")
    return (
      <div className="tile claude">
        <span className="dim">Claude · {u.kind === "expired" ? "Claude Code'u aç" : "bilinmiyor"}</span>
      </div>
    );
  return (
    <div className="tile claude" title={`5 sa: %${Math.round(u.fiveHour)} · Haftalık: %${Math.round(u.weekly)}`}>
      <span className="dim">Claude</span>
      <div style={{ width: 60, display: "grid", gap: 4 }}>
        <Bar pct={u.fiveHour} color="#f5a524" />
        <Bar pct={u.weekly} color="#8b5cf6" />
      </div>
    </div>
  );
}
