import { useEffect, useState } from "react";
import { EVENTS, langList, langSelect, useEvent, type Layout } from "../lib/ipc";

const SPEAKER = "M11 5 6 9H3v6h3l5 4V5zM15.5 8.5a5 5 0 0 1 0 7M18.5 5.5a9 9 0 0 1 0 13";
/** Row icons (24 x 24 strokes) by the key the backend sends as "i:<key>". */
const ICON: Record<string, string> = {
  apps: "M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z",
  mobility: "M5 6h14v9H5zM3 19h18",
  power: "M13 3 6 13h5l-1 8 8-11h-5z",
  events: "M7 3h10v18H7zM10 8h4M10 12h4M10 16h4",
  system: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 11v6M12 7.5v.01",
  devices: "M8 8h8v8H8zM10 3v3M14 3v3M10 18v3M14 18v3M3 10h3M3 14h3M18 10h3M18 14h3",
  network: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM3 12h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18",
  disks: "M4 14h16v5H4zM4 14l3-8h10l3 8M16.5 16.5v.01",
  compmgmt: "M4 7h10M18 7h2M4 17h2M10 17h10M16 5v4M8 15v4",
  term: "M4 5h16v14H4zM7 10l3 2-3 2M12 15h5",
  termadmin: "M12 3 5 6v5c0 5 3 8 7 10 4-2 7-5 7-10V6zM9.5 12 11 13.5 14.5 10",
  taskmgr: "M3 20h18M6 20v-6M11 20V6M16 20v-9",
  settings: "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19 12a7 7 0 0 0-.1-1.2l2-1.5-2-3.5-2.3 1a7 7 0 0 0-2-1.2L14.2 3h-4l-.4 2.6a7 7 0 0 0-2 1.2l-2.3-1-2 3.5 2 1.5a7 7 0 0 0 0 2.4l-2 1.5 2 3.5 2.3-1a7 7 0 0 0 2 1.2l.4 2.6h4l.4-2.6a7 7 0 0 0 2-1.2l2.3 1 2-3.5-2-1.5c.1-.4.1-.8.1-1.2z",
  explorer: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
  run: "M4 5h16v14H4zM10 9l5 3-5 3z",
  desktop: "M3 5h18v11H3zM8 20h8M12 16v4",
  one: "M6 6l12 12M18 6 6 18",
  all: "M5 8h11v11H5zM8 5h11v11M8.5 11.5l4 4M12.5 11.5l-4 4",
  new: "M12 5v14M5 12h14",
  doc: "M7 3h7l4 4v14H7zM14 3v4h4M10 12h5M10 16h5",
  clip: "M9 4h6v3H9zM8 5.5H6.5A1.5 1.5 0 0 0 5 7v12a1.5 1.5 0 0 0 1.5 1.5h11A1.5 1.5 0 0 0 19 19V7a1.5 1.5 0 0 0-1.5-1.5H16M9 12h6M9 16h4",
  clear: "M4 7h16M10 11v6M14 11v6M6 7l1 12.2A2 2 0 0 0 9 21h6a2 2 0 0 0 2-1.8L18 7M9 7V4.5A1.5 1.5 0 0 1 10.5 3h3A1.5 1.5 0 0 1 15 4.5V7",
  pin: "M12 17v5M9 3h6l-1 6 3 3H7l3-3z",
  unpin: "M12 17v5M9 3h6l-1 6 3 3H7l3-3zM4 4l16 16",
  end: "M12 3v9M7.5 6.5a7.5 7.5 0 1 0 9 0",
};

/** The keyboard layout picker window (above the dock). */
const SEP_BEFORE = new Set(["pin", "unpin", "taskmgr", "desktop", "recent:0", "clear"]);
const DANGER = new Set(["end", "clear"]);

export function LangPopup() {
  const openEvt = useEvent<boolean>(EVENTS.langOpen, false);
  const [open, setOpen] = useState(false);
  const [layouts, setLayouts] = useState<Layout[]>([]);
  useEffect(() => {
    // Two frames after the window is shown, so the CSS animation starts from the hidden state.
    if (!openEvt) return setOpen(false);
    void langList().then(setLayouts);
    const a = requestAnimationFrame(() => requestAnimationFrame(() => setOpen(true)));
    return () => cancelAnimationFrame(a);
  }, [openEvt]);
  return (
    <div className={open ? "lp in" : "lp"}>
      {layouts.map((l) => (
        <button key={l.hkl} className={`lp-row${l.current ? " cur" : ""}${SEP_BEFORE.has(l.hkl) ? " grp" : ""}${DANGER.has(l.hkl) ? " danger" : ""}`} onClick={() => void langSelect(l.hkl)}>
          <span className="lp-code">
            {l.code.startsWith("img:") ? (
              <img src={l.code.slice(4)} alt="" draggable={false} />
            ) : l.code.startsWith("i:") || !l.code ? (
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                <path d={ICON[l.code.slice(2)] ?? SPEAKER} />
              </svg>
            ) : (
              l.code
            )}
          </span>
          <span className="lp-name">{l.name}</span>
          {l.current && (
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
              <path d="M5 12.5l4.5 4.5L19 7.5" />
            </svg>
          )}
        </button>
      ))}
    </div>
  );
}
