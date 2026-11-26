import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { batteryList, btGet, setBatteryPref, type BtDevice, type DeviceBattery } from "../lib/ipc";
import { batteryShown, batteryUndecided, btKey } from "../lib/batteryPrefs";
import { BtIcon } from "../widgets/BtIcons";

/** The devices of the battery widget: the ones it reads itself, and the Bluetooth devices it finds; each can be shown or not. */
export function BatteryDevices({ prefs }: { prefs: Record<string, boolean> }) {
  const [own, setOwn] = useState<DeviceBattery[]>([]);
  const [bt, setBt] = useState<BtDevice[]>([]);
  useEffect(() => {
    let live = true;
    const load = () => {
      void batteryList().then((l) => live && setOwn(l));
      void btGet().then((s) => live && setBt(s.devices));
    };
    load();
    const t = setInterval(load, 5000);
    const un = listen("battery://changed", load);
    return () => {
      live = false;
      clearInterval(t);
      void un.then((f) => f());
    };
  }, []);

  const fresh = bt.filter((d) => d.battery != null && batteryUndecided(prefs, btKey(d.id)));
  const toggle = (id: string, label: string, sub: string, icon: React.ReactNode) => (
    <label className="batdev" key={id}>
      <span className="batdev-ic">{icon}</span>
      <span className="batdev-t">
        <b>{label}</b>
        <em>{sub}</em>
      </span>
      <input type="checkbox" checked={batteryShown(prefs, id)} onChange={(e) => void setBatteryPref(id, e.target.checked)} />
    </label>
  );
  return (
    <div className="batdevs">
      <span className="batdevs-h">Gösterilecek cihazlar</span>
      {fresh.map((d) => (
        <div className="batdev found" key={`new-${d.id}`}>
          <span className="batdev-ic"><BtIcon kind={d.icon} size={16} /></span>
          <span className="batdev-t">
            <b>{d.name} algılandı</b>
            <em>Şarjı %{d.battery}. Dock'ta gösterilsin mi?</em>
          </span>
          <button className="keybtn rec" onClick={() => void setBatteryPref(btKey(d.id), true)}>Göster</button>
          <button className="keybtn" onClick={() => void setBatteryPref(btKey(d.id), false)}>Gösterme</button>
        </div>
      ))}
      {own.map((d) => toggle(d.id, d.name, d.connected ? (d.percent != null ? `%${d.percent}` : "bağlı") : "bağlı değil", <span className="batdev-dot" />))}
      {bt
        .filter((d) => !fresh.some((f) => f.id === d.id))
        .map((d) => toggle(btKey(d.id), d.name, d.connected ? (d.battery != null ? `%${d.battery} · Bluetooth` : "Bluetooth · şarj bilgisi yok") : "Bluetooth · bağlı değil", <BtIcon kind={d.icon} size={16} />))}
      {own.length === 0 && bt.length === 0 && <span className="batdevs-empty">Cihaz bulunamadı.</span>}
    </div>
  );
}
