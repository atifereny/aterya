import { useRef } from "react";
import { EVENTS, mixerList, mixerToggle, openQuickSettings, outdevToggle, setVolume, useEvent, type SystemState } from "../lib/ipc";
import { stepVolume } from "../lib/clock";
import { NetworkIcon, SpeakerIcon } from "./icons";

const NET_LABEL = { ethernet: "Ethernet", wifi: "Wi-Fi", offline: "Çevrimdışı" } as const;

export function SystemControls() {
  const s = useEvent<SystemState>(EVENTS.system, { volume: 0, muted: false, network: "offline" });
  const vol = useRef<HTMLSpanElement>(null);
  const mixerOpen = useEvent<boolean>(EVENTS.mixerOpen, false);
  const openAtDown = useRef(false);
  const langAtDown = useRef(false);
  const langOpen = useEvent<boolean>(EVENTS.langOpen, false);
  const openMixer = async () => {
    const r = vol.current!.getBoundingClientRect();
    await mixerToggle(r.left + r.width / 2, openAtDown.current ? 0 : (await mixerList()).length, openAtDown.current);
  };
  return (
    <div className="tile system">
      <span
        ref={vol}
        onPointerDown={() => { openAtDown.current = mixerOpen; langAtDown.current = langOpen; }}
        className="big sys-btn"
        title={`Ses %${s.volume}`}
        onClick={() => void openMixer()}
        onContextMenu={(e) => {
          e.preventDefault();
          e.stopPropagation();
          const r = vol.current!.getBoundingClientRect();
          void outdevToggle(r.left + r.width / 2, langAtDown.current);
        }}
        onWheel={(e) => void setVolume(stepVolume(s.volume, e.deltaY))}
      >
        <SpeakerIcon level={s.muted || s.volume === 0 ? "mute" : s.volume < 50 ? "low" : "high"} />
        <span className="dim">{s.volume}</span>
      </span>
      <span
        className={s.network === "offline" ? "big sys-btn net-off" : "big sys-btn"}
        title={NET_LABEL[s.network]}
        onClick={() => void openQuickSettings()}
      >
        <NetworkIcon kind={s.network} />
        {s.network === "offline" && <span className="net-label">Bağlantı yok</span>}
      </span>
    </div>
  );
}
