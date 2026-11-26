import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { activateWindow, closeWindow, previewEnter, previewItems, previewLeave, PreviewPayload } from "../lib/ipc";

// The thumbnail slot under each title row stays empty: Rust draws a DWM thumbnail there, above this page.
export function Preview() {
  const [p, setP] = useState<PreviewPayload | null>(null);
  useEffect(() => {
    const un = listen<PreviewPayload>("preview://items", (e) => setP(e.payload));
    void previewItems().then((v) => v && setP((cur) => cur ?? v)); // first show can precede the listener
    return () => void un.then((f) => f());
  }, []);
  if (!p) return null;
  return (
    <div className="pv" onMouseEnter={() => void previewEnter()} onMouseLeave={() => void previewLeave()}>
      {p.items.map((it) => (
        <div
          key={it.hwnd}
          className="pv-cell"
          style={{ left: it.x, top: it.y, width: p.thumbW, height: p.titleH + p.thumbH }}
          onClick={() => void activateWindow(it.hwnd)}
          onMouseDown={(e) => e.button === 1 && e.preventDefault()} // no autoscroll cursor on a middle click
          onAuxClick={(e) => {
            // Middle button closes the window, like on a browser tab.
            if (e.button !== 1) return;
            e.preventDefault();
            void closeWindow(it.hwnd);
          }}
        >
          <div className="pv-title" style={{ height: p.titleH }}>
            <span title={it.title}>{it.title || "(untitled)"}</span>
            <button aria-label="Close window" onClick={(e) => { e.stopPropagation(); void closeWindow(it.hwnd); }}>✕</button>
          </div>
          {it.minimized && (
            // A minimized window cannot be drawn live: its last picture, or its icon if none was taken yet.
            <div className="pv-min" style={{ height: p.thumbH }}>
              {it.snap ? <img className="pv-snap" src={it.snap} alt="" draggable={false} /> : p.icon && <img src={p.icon} width={48} height={48} alt="" draggable={false} />}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
