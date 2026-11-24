import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { btGet, btToggle, EVENTS, useEvent, type BtState } from "../lib/ipc";
import { along } from "../lib/axis";
import { BtIcon, batColor } from "./BtIcons";

const EMPTY: BtState = { available: false, on: false, devices: [] };

/** Bluetooth state: refreshed now and then, and at once when the popup changes something. */
export function useBluetooth(): BtState {
  const [s, setS] = useState<BtState>(EMPTY);
  useEffect(() => {
    let live = true;
    const load = () => void btGet().then((n) => live && setS((o) => (JSON.stringify(o) === JSON.stringify(n) ? o : n)));
    load();
    const t = setInterval(load, 10000);
    const un = listen("bt://changed", () => setTimeout(load, 400));
    return () => {
      live = false;
      clearInterval(t);
      void un.then((f) => f());
    };
  }, []);
  return s;
}

function useOpenToggle() {
  const open = useEvent<boolean>(EVENTS.btOpen, false);
  const atDown = useRef(false);
  return { down: () => { atDown.current = open; }, atDown };
}

/** Bluetooth switch state and the devices the user chose to show: their icon and battery. */
export function Bluetooth() {
  const s = useBluetooth();
  const ref = useRef<HTMLDivElement>(null);
  const { down, atDown } = useOpenToggle();
  const shown = s.devices.filter((d) => d.show && d.connected);
  return (
    <div
      ref={ref}
      className={s.on ? "tile btw" : "tile btw off"}
      title={!s.available ? "Bluetooth bulunamadı" : s.on ? "Bluetooth açık · tıkla: cihazlar" : "Bluetooth kapalı · tıkla: aç/kapat"}
      onPointerDown={down}
      onClick={() => void btToggle(along(ref.current!.getBoundingClientRect()), 260, atDown.current)}
    >
      <span className="bt-main"><BtIcon kind="bluetooth" size={17} /></span>
      {s.on &&
        shown.map((d) => (
          <span className="bt-dev" key={d.id} title={`${d.name}${d.battery != null ? ` %${d.battery}` : ""}`}>
            <BtIcon kind={d.icon} size={15} />
            {d.battery != null && <b style={{ color: batColor(d.battery) }}>{d.battery}%</b>}
          </span>
        ))}
    </div>
  );
}

/** Column version for a dock on a side edge: the switch icon, then one icon with its battery per device. */
export function SideBluetooth() {
  const s = useBluetooth();
  const ref = useRef<HTMLDivElement>(null);
  const { down, atDown } = useOpenToggle();
  const shown = s.devices.filter((d) => d.show && d.connected);
  return (
    <div ref={ref} className={s.on ? "tile side btw" : "tile side btw off"} title={s.on ? "Bluetooth açık" : "Bluetooth kapalı"} onPointerDown={down} onClick={() => void btToggle(along(ref.current!.getBoundingClientRect()), 260, atDown.current)}>
      <BtIcon kind="bluetooth" size={17} />
      {s.on &&
        shown.map((d) => (
          <span className="bt-dev" key={d.id} title={d.name}>
            <BtIcon kind={d.icon} size={14} />
            {d.battery != null && <b style={{ color: batColor(d.battery) }}>{d.battery}</b>}
          </span>
        ))}
    </div>
  );
}
