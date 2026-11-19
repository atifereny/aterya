import { useEffect, useRef } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { floatPin, requestState, useSettings, type WidgetId } from "../lib/ipc";
import { WIDGET_INFO } from "../lib/widgets";
import { Zoom } from "../lib/Zoom";
import { applyLook } from "../lib/look";

const PAD = 12; // room around the pill for the hover lift

const Pin = ({ on }: { on: boolean }) => (
  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
    <rect x="5" y="11" width="14" height="9" rx="2.5" fill={on ? "currentColor" : "none"} />
    <path d={on ? "M8 11V8a4 4 0 0 1 8 0v3" : "M8 11V8a4 4 0 0 1 7.6-1.7"} />
  </svg>
);

const Grip = () => (
  <svg width="22" height="10" viewBox="0 0 22 10" fill="currentColor" aria-hidden data-tauri-drag-region>
    {[3, 8, 13, 18].flatMap((x) => [2, 7].map((y) => <circle key={`${x}-${y}`} cx={x} cy={y} r="1.15" data-tauri-drag-region />))}
  </svg>
);

/** A widget living on its own: move it by the handle, then pin it in place. */
export function Float() {
  const id = location.hash.replace("#float=", "") as WidgetId;
  const s = useSettings();
  const pill = useRef<HTMLDivElement>(null);
  const pinned = s?.floating.find((f) => f.id === id)?.pinned ?? false;
  useEffect(() => {
    if (!s) return;
    document.documentElement.dataset.theme = s.theme.preset;
    document.documentElement.style.setProperty("--fill", String(s.theme.opacity));
    applyLook(s);
  }, [s]);
  useEffect(() => {
    const el = pill.current;
    if (!el) return;
    const fit = () => {
      const r = el.getBoundingClientRect();
      void getCurrentWindow().setSize(new LogicalSize(Math.ceil(r.width) + PAD * 2, Math.ceil(r.height) + PAD * 2));
    };
    const ro = new ResizeObserver(fit);
    ro.observe(el);
    const replay = setTimeout(() => void requestState(), 300); // widgets mounted late missed the backend events
    return () => {
      ro.disconnect();
      clearTimeout(replay);
    };
  }, [s === null]);
  if (!s || !WIDGET_INFO[id]) return null;
  return (
    <div className="fl-wrap">
      <div className="fl" ref={pill}>
        <div className="fl-bar" data-tauri-drag-region={pinned ? undefined : ""} title={pinned ? "Sabitlendi" : "Sürükleyerek taşı"}>
          {!pinned && <span className="fl-grip" data-tauri-drag-region><Grip /></span>}
          <button className={pinned ? "fl-pin on" : "fl-pin"} onClick={() => void floatPin(id, !pinned)} aria-label={pinned ? "Sabitlemeyi kaldır" : "Sabitle"} title={pinned ? "Sabitlemeyi kaldır" : "Buraya sabitle"}>
            <Pin on={pinned} />
            {pinned && <span>Sabit</span>}
          </button>
        </div>
        <Zoom scale={s.scales[id]}>{WIDGET_INFO[id].render()}</Zoom>
      </div>
    </div>
  );
}
