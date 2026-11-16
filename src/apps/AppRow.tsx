import { appClick, AppEntry, EVENTS, startMenu, useEvent } from "../lib/ipc";
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
          onClick={() => void appClick(a.id)}
        >
          <img src={a.iconDataUrl} width={size} height={size} alt={a.name} draggable={false} />
        </div>
      ))}
    </div>
  );
}
