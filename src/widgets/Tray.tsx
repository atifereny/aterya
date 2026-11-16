import { EVENTS, openTrayOverflow, trayClick, useEvent, type TrayIcon } from "../lib/ipc";

// Fallback path: icons is empty (no tray host), the chevron opens Windows' overflow flyout.
export function Tray() {
  const icons = useEvent<TrayIcon[]>(EVENTS.tray, []);
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
      <button className="tray-chevron" title="Show hidden icons" onClick={() => void openTrayOverflow()}>⌃</button>
    </div>
  );
}
