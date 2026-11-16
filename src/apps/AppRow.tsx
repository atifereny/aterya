import { useState } from "react";
import { appClick, AppEntry, EVENTS, previewHover, previewLeave, startMenu, useEvent } from "../lib/ipc";
import { ICON_GAP, iconSizeFor } from "../lib/fit";

export function AppRow({ availablePx }: { availablePx: number }) {
  const apps = useEvent<AppEntry[]>(EVENTS.apps, []);
  const size = iconSizeFor(apps.length + 1, availablePx);
  const box = { width: size + 12, height: size + 12 };
  return (
    <div className="app-row" style={{ display: "flex", alignItems: "center", gap: ICON_GAP }}>
      <div className="app" style={box} title="Start" onClick={() => void startMenu()}>
        <svg width={size * 0.8} height={size * 0.8} viewBox="0 0 16 16" fill="currentColor">
          <path d="M0 2.3 6.5 1.4v5.8H0zM7.4 1.3 16 0v7.2H7.4zM0 8.1h6.5v5.8L0 13zM7.4 8.1H16V16l-8.6-1.2z" />
        </svg>
      </div>
      {apps.map((a) => (
        <div
          key={a.id}
          className={`app${a.running ? " on" : ""}`}
          style={{ ...box, ...(a.focused ? { ["--dot" as string]: "1" } : {}) }}
          title={a.name}
          onMouseEnter={(e) => {
            if (a.windowCount > 0) {
              const r = e.currentTarget.getBoundingClientRect();
              void previewHover(a.id, r.left + r.width / 2);
            }
          }}
          onMouseLeave={() => void previewLeave()}
          onClick={() => {
            void previewLeave();
            void appClick(a.id);
          }}
        >
          <AppIcon app={a} size={size} />
        </div>
      ))}
    </div>
  );
}

// Empty or undecodable icon: a letter tile instead of the browser's alt text.
function AppIcon({ app, size }: { app: AppEntry; size: number }) {
  const [bad, setBad] = useState(false);
  if (!app.iconDataUrl || bad)
    return (
      <div style={{ width: size, height: size, borderRadius: 8, background: "rgba(255,255,255,0.22)", display: "grid", placeItems: "center", fontWeight: 700, fontSize: size * 0.5 }}>
        {[...app.name][0]?.toUpperCase() ?? "?"}
      </div>
    );
  return <img src={app.iconDataUrl} width={size} height={size} alt="" draggable={false} onError={() => setBad(true)} />;
}
