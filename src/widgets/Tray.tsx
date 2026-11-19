import { useRef } from "react";
import { EVENTS, openTrayOverflow, trayClick, useEvent, type TrayIcon } from "../lib/ipc";

// Fallback path: icons is empty (no tray host), the chevron opens Windows' overflow flyout.
export function Tray() {
  const icons = useEvent<TrayIcon[]>(EVENTS.tray, []);
  const open = useEvent<boolean>(EVENTS.trayFlyout, false);
  const openAtDown = useRef(false);
  const click = (id: string) => (e: React.MouseEvent) => {
    const dpr = window.devicePixelRatio;
    void trayClick(id, e.button === 2 ? "right" : "left", Math.round(e.screenX * dpr), Math.round(e.screenY * dpr));
  };
  return (
    <div className="tray">
      {icons.map((t) => (
        <img
          key={t.id}
          src={t.iconDataUrl}
          title={t.tooltip}
          width={16}
          height={16}
          onClick={click(t.id)}
          onContextMenu={(e) => { e.preventDefault(); click(t.id)(e); }}
        />
      ))}
      <button
        className={open ? "tray-chevron open" : "tray-chevron"}
        onPointerDown={() => { openAtDown.current = open; }}
        aria-label="Gizli simgeler"
        onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          const d = window.devicePixelRatio;
          void openTrayOverflow(Math.round((window.screenX + r.left + r.width / 2) * d), Math.round((window.screenY + r.top) * d), openAtDown.current);
        }}
      >
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M3 9l4-4 4 4" /></svg>
      </button>
    </div>
  );
}
