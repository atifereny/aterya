import { useEffect, useRef, useState } from "react";
import { appClick, appMenuToggle, tipHide, tipShow, AppEntry, EVENTS, previewHover, previewLeave, startMenu, useEvent, useSettings } from "../lib/ipc";
import { StartIcon } from "../lib/StartIcon";
import { ICON_GAP, iconSizeFor } from "../lib/fit";

const MAX_DOTS = 4; // one dot per open window, capped so the row stays tidy

type Row = { app: AppEntry; state: "" | "in" | "leaving" };
const LEAVE_MS = 300;

/** The apps with enter and leave states, so an app joining the dock grows in and one leaving shrinks away instead of popping. */
function useAnimatedApps(apps: AppEntry[]): Row[] {
  const [rows, setRows] = useState<Row[]>(() => apps.map((app) => ({ app, state: "" })));
  const ready = useRef(false); // the first list (dock start) appears without animation
  const prev = useRef<AppEntry[]>(apps);
  useEffect(() => {
    const t = setTimeout(() => (ready.current = true), 1500);
    return () => clearTimeout(t);
  }, []);
  useEffect(() => {
    const old = prev.current;
    prev.current = apps;
    const ids = new Set(apps.map((a) => a.id));
    const oldIds = new Set(old.map((a) => a.id));
    setRows((cur) => {
      const out: Row[] = apps.map((app) => {
        const was = cur.find((r) => r.app.id === app.id);
        return { app, state: was && was.state !== "leaving" ? was.state : ready.current && !oldIds.has(app.id) ? "in" : "" };
      });
      if (ready.current) {
        cur.forEach((r, i) => {
          if (!ids.has(r.app.id)) out.splice(Math.min(i, out.length), 0, { app: r.app, state: "leaving" });
        });
      }
      return out;
    });
    const t = setTimeout(() => setRows((cur) => cur.filter((r) => r.state !== "leaving" || ids.has(r.app.id))), LEAVE_MS + 40);
    return () => clearTimeout(t);
  }, [apps]);
  return rows;
}

export function AppRow({ availablePx }: { availablePx: number }) {
  const cfg = useSettings();
  const indicator = cfg?.indicator || "dot";
  const startIcon = cfg?.startIcon || "windows";
  const apps = useEvent<AppEntry[]>(EVENTS.apps, []);
  const rows = useAnimatedApps(apps);
  const menuOpen = useEvent<boolean>(EVENTS.langOpen, false);
  const menuAtDown = useRef(false);
  const tipTimer = useRef(0);
  const showTip = (text: string, sub: string, icon: string, el: HTMLElement) => {
    window.clearTimeout(tipTimer.current);
    const r = el.getBoundingClientRect();
    tipTimer.current = window.setTimeout(() => void tipShow(text, sub, icon, r.left + r.width / 2), 380);
  };
  const hideTip = () => {
    window.clearTimeout(tipTimer.current);
    void tipHide();
  };
  const gap = cfg?.iconGap ?? ICON_GAP;
  const size = iconSizeFor(apps.length + 1, availablePx, cfg?.iconSize, gap);
  const box = { width: size + 12, height: size + 12 };
  return (
    <div className="app-row" data-apps style={{ display: "flex", alignItems: "center", gap }}>
      <div className="app" style={box} onMouseEnter={(e) => showTip("Başlat", "Windows", "", e.currentTarget)} onMouseLeave={hideTip} onClick={() => { hideTip(); void startMenu(); }}
        onPointerDown={() => { menuAtDown.current = menuOpen; }}
        onContextMenu={(e) => {
          e.preventDefault();
          e.stopPropagation();
          const r = e.currentTarget.getBoundingClientRect();
          void appMenuToggle(r.left + r.width / 2, "winx", menuAtDown.current);
        }}>
        <StartIcon kind={startIcon} size={size * 0.8} />
      </div>
      {rows.map(({ app: a, state }) => (
        <div
          key={a.id}
          className={`app${a.attention ? " attn" : ""}${state === "in" ? " enter" : state === "leaving" ? " leaving" : ""}`}
          style={box}
          onMouseEnter={(e) => {
            if (a.windowCount > 0) {
              const r = e.currentTarget.getBoundingClientRect();
              void previewHover(a.id, r.left + r.width / 2);
            } else {
              showTip(a.name, a.pinned ? "Çalışmıyor · açmak için tıkla" : "Açmak için tıkla", a.iconDataUrl, e.currentTarget);
            }
          }}
          onMouseLeave={() => {
            hideTip();
            void previewLeave();
          }}
          onPointerDown={() => { menuAtDown.current = menuOpen; }}
          onContextMenu={(e) => {
            e.preventDefault();
            e.stopPropagation(); // the dock bar behind opens the settings on right-click
            void previewLeave();
            const r = e.currentTarget.getBoundingClientRect();
            void appMenuToggle(r.left + r.width / 2, a.id, menuAtDown.current);
          }}
          onClick={() => {
            hideTip();
            void previewLeave();
            void appClick(a.id);
          }}
        >
          <AppIcon app={a} size={size} />
          {a.badge > 0 && <b className="badge">{a.badge > 99 ? "99+" : a.badge}</b>}
          {a.windowCount > 0 && (
            <div className={`${a.focused ? "dots focused" : "dots"} ind-${indicator}`}>
              {Array.from({ length: Math.min(a.windowCount, MAX_DOTS) }, (_, i) => (
                <i key={i} />
              ))}
            </div>
          )}
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
