import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { btFit, btGet, btSetPower, btSetPref, EVENTS, useEvent, type BtDevice, type BtState } from "../lib/ipc";
import { BT_ICONS, BtIcon, batColor } from "../widgets/BtIcons";

const EMPTY: BtState = { available: false, on: false, devices: [] };

function Device({ d, open, onOpen, onPatch }: { d: BtDevice; open: boolean; onOpen: () => void; onPatch: (p: Partial<BtDevice>) => void }) {
  const set = (show: boolean, icon: string) => {
    onPatch({ show, icon }); // shown at once; the saved state follows
    void btSetPref(d.id, show, icon);
  };
  return (
    <div className="bt-item">
      <div className={d.connected ? "mx-row bt-row" : "mx-row bt-row off"}>
        <button className="mx-mute" onClick={onOpen} aria-label="Simge seç" title="Simgeyi değiştir">
          <BtIcon kind={d.icon} size={20} />
        </button>
        <div className="mx-body">
          <div className="mx-name">
            <span>{d.name}</span>
            <span className="dim">{d.connected ? "Bağlı" : "Bağlı değil"}</span>
          </div>
          <div className="bt-bar" title={d.battery != null ? `%${d.battery}` : "Şarj bilgisi yok"}>
            <i style={{ width: `${d.battery ?? 0}%`, background: d.battery != null ? batColor(d.battery) : "transparent" }} />
          </div>
        </div>
        <span className="bt-pct" style={{ color: d.battery != null ? batColor(d.battery) : undefined }}>{d.battery != null ? `${d.battery}%` : "—"}</span>
        <button className={d.show ? "bt-eye on" : "bt-eye"} onClick={() => set(!d.show, d.icon)} aria-label={d.show ? "Dock'tan kaldır" : "Dock'ta göster"} title={d.show ? "Dock'ta gösteriliyor" : "Dock'ta göster"}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
            <path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z" />
            <circle cx="12" cy="12" r="3" />
            {!d.show && <path d="M4 4l16 16" />}
          </svg>
        </button>
      </div>
      {open && (
        <div className="bt-icons">
          {Object.entries(BT_ICONS).map(([k, v]) => (
            <button key={k} className={k === d.icon ? "bt-ic sel" : "bt-ic"} title={v.label} onClick={() => set(d.show, k)}>
              <BtIcon kind={k} size={18} />
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export function BtPopup() {
  const openEvt = useEvent<boolean>(EVENTS.btOpen, false);
  const [open, setOpen] = useState(false);
  useEffect(() => {
    if (!openEvt) return setOpen(false);
    const a = requestAnimationFrame(() => requestAnimationFrame(() => setOpen(true)));
    return () => cancelAnimationFrame(a);
  }, [openEvt]);
  const [fallback, setFallback] = useState(false);
  useEffect(() => {
    const t = setTimeout(() => setFallback(true), 400);
    return () => clearTimeout(t);
  }, []);
  const seenEvent = useRef(false);
  if (openEvt) seenEvent.current = true;

  const [s, setS] = useState<BtState>(EMPTY);
  const [picking, setPicking] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let live = true;
    const load = () => void btGet().then((n) => live && setS((o) => (JSON.stringify(o) === JSON.stringify(n) ? o : n)));
    load();
    const t = setInterval(load, openEvt ? 2000 : 15000);
    const un = listen("bt://changed", () => setTimeout(load, 300));
    return () => {
      live = false;
      clearInterval(t);
      void un.then((f) => f());
    };
  }, [openEvt]);
  useEffect(() => {
    if (!openEvt) setPicking(null);
  }, [openEvt]);

  // The window is as tall as its content plus the popup's own padding.
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const ro = new ResizeObserver(() => void btFit(Math.ceil(el.getBoundingClientRect().height) + 20));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const toggle = async () => {
    if (busy || !s.available) return;
    setBusy(true);
    setS((o) => ({ ...o, on: !o.on }));
    try {
      await btSetPower(!s.on);
    } catch {
      /* the next refresh shows the real state */
    }
    setBusy(false);
  };

  return (
    <div className={open || (fallback && !seenEvent.current) ? "mx bt in" : "mx bt"}>
      <div ref={box} className="bt-box">
        <div className="mx-row mx-master">
          <span className="mx-mute bt-logo"><BtIcon kind="bluetooth" size={20} /></span>
          <div className="mx-body">
            <div className="mx-name">
              <span>Bluetooth</span>
              <span className="dim">{!s.available ? "Yok" : s.on ? "Açık" : "Kapalı"}</span>
            </div>
          </div>
          <button className={s.on ? "bt-sw on" : "bt-sw"} onClick={() => void toggle()} disabled={!s.available} aria-label="Bluetooth'u aç veya kapat" role="switch" aria-checked={s.on}>
            <i />
          </button>
        </div>
        <div className="mx-sep" />
        {!s.available && <div className="dim mx-empty">Bu bilgisayarda Bluetooth bulunamadı</div>}
        {s.available && !s.on && <div className="dim mx-empty">Bluetooth kapalı</div>}
        {s.on && s.devices.length === 0 && <div className="dim mx-empty">Eşleşmiş cihaz yok</div>}
        {s.on && s.devices.map((d) => <Device key={d.id} d={d} open={picking === d.id} onOpen={() => setPicking((p) => (p === d.id ? null : d.id))} onPatch={(p) => setS((o) => ({ ...o, devices: o.devices.map((x) => (x.id === d.id ? { ...x, ...p } : x)) }))} />)}
        {s.on && s.devices.length > 0 && <div className="dim bt-hint">Simgeye bas: simgeyi seç · Göz: dock'ta göster</div>}
      </div>
    </div>
  );
}
