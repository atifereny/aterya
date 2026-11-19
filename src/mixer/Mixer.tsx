import { useEffect, useRef, useState } from "react";
import { EVENTS, mixerFit, mixerList, mixerSetMute, mixerSetVolume, requestState, setVolume, toggleMute, useEvent, type MixerApp, type SystemState } from "../lib/ipc";
import { SpeakerIcon } from "../widgets/icons";

interface RowProps {
  icon?: string;
  name: string;
  volume: number;
  muted: boolean;
  onVolume: (v: number) => void;
  onMute: () => void;
  master?: boolean;
}

function Row({ icon, name, volume, muted, onVolume, onMute, master }: RowProps) {
  const [drag, setDrag] = useState<number | null>(null); // local value while the thumb is held, so polling cannot fight it
  const shown = drag ?? volume;
  return (
    <div className={master ? "mx-row mx-master" : "mx-row"}>
      <button className="mx-mute" onClick={onMute} aria-label={muted ? "Sesi aç" : "Sustur"}>
        {icon ? (
          <img src={icon} width={22} height={22} alt="" draggable={false} style={{ opacity: muted ? 0.4 : 1 }} />
        ) : (
          <SpeakerIcon level={muted ? "mute" : shown < 50 ? "low" : "high"} size={22} />
        )}
      </button>
      <div className="mx-body">
        <div className="mx-name">
          <span>{name}</span>
          <span className="dim">{muted ? "Sessiz" : shown}</span>
        </div>
        <input
          type="range"
          min={0}
          max={100}
          value={shown}
          style={{ ["--p" as string]: `${muted ? 0 : shown}%` }}
          onChange={(e) => {
            const v = Number(e.target.value);
            setDrag(v);
            onVolume(v);
          }}
          onPointerUp={() => setDrag(null)}
          onKeyUp={() => setDrag(null)}
        />
      </div>
    </div>
  );
}

export function Mixer() {
  const sys = useEvent<SystemState>(EVENTS.system, { volume: 0, muted: false, network: "offline" });
  const openEvt = useEvent<boolean>(EVENTS.mixerOpen, false);
  const [open, setOpen] = useState(false);
  useEffect(() => {
    // Two frames after the window is shown, so the CSS animation starts from the hidden state.
    if (!openEvt) return setOpen(false);
    const a = requestAnimationFrame(() => requestAnimationFrame(() => setOpen(true)));
    return () => cancelAnimationFrame(a);
  }, [openEvt]);
  const [fallback, setFallback] = useState(false); // very first show can precede the listener
  useEffect(() => {
    const t = setTimeout(() => setFallback(true), 400);
    return () => clearTimeout(t);
  }, []);
  const [apps, setApps] = useState<MixerApp[]>([]);
  const rows = useRef(-1);
  const seenEvent = useRef(false);
  if (openEvt) seenEvent.current = true;
  useEffect(() => {
    const replay = setTimeout(() => void requestState(), 300); // master volume arrives only as an event
    return () => clearTimeout(replay);
  }, []);
  useEffect(() => {
    let live = true;
    const tick = async () => {
      const l = await mixerList();
      if (!live) return;
      setApps(l);
      if (l.length !== rows.current) {
        rows.current = l.length;
        void mixerFit(l.length);
      }
    };
    void tick();
    const t = setInterval(() => void tick(), 1000);
    return () => {
      live = false;
      clearInterval(t);
    };
  }, []);
  const patch = (id: string, p: Partial<MixerApp>) => setApps((a) => a.map((x) => (x.id === id ? { ...x, ...p } : x)));
  return (
    <div className={open || (fallback && !seenEvent.current) ? "mx in" : "mx"}>
      <Row name="Ana ses" volume={sys.volume} muted={sys.muted} master onVolume={(v) => void setVolume(v)} onMute={() => void toggleMute()} />
      <div className="mx-sep" />
      {apps.length === 0 && <div className="dim mx-empty">Ses çalan uygulama yok</div>}
      {apps.map((a) => (
        <Row
          key={a.id}
          icon={a.icon}
          name={a.name}
          volume={a.volume}
          muted={a.muted}
          onVolume={(v) => {
            patch(a.id, { volume: v });
            void mixerSetVolume(a.id, v);
          }}
          onMute={() => {
            patch(a.id, { muted: !a.muted });
            void mixerSetMute(a.id, !a.muted);
          }}
        />
      ))}
    </div>
  );
}
