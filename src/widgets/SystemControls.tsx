import { EVENTS, openQuickSettings, setVolume, toggleMute, useEvent, type SystemState } from "../lib/ipc";
import { stepVolume } from "../lib/clock";

const NET_LABEL = { ethernet: "Ethernet", wifi: "Wi-Fi", offline: "Çevrimdışı" } as const;
const NET_ICON = { ethernet: "🖧", wifi: "📶", offline: "⚠" } as const;

export function SystemControls() {
  const s = useEvent<SystemState>(EVENTS.system, { volume: 0, muted: false, network: "offline" });
  return (
    <div className="tile system">
      <span
        className="big"
        title={`Ses %${s.volume}`}
        onClick={() => void toggleMute()}
        onWheel={(e) => void setVolume(stepVolume(s.volume, e.deltaY))}
      >
        {s.muted || s.volume === 0 ? "🔇" : "🔊"} <span className="dim">{s.volume}</span>
      </span>
      <span className="big" title={NET_LABEL[s.network]} onClick={() => void openQuickSettings()}>
        {NET_ICON[s.network]}
      </span>
    </div>
  );
}
