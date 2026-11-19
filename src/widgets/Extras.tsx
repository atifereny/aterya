import { useEffect, useRef, useState } from "react";
import { appMenuToggle, binEmpty, binGet, binOpen, clipCount, EVENTS, micGet, micToggle, noteNew, notesCount, useEvent, type BinState, type MicState } from "../lib/ipc";

const svg = { width: 20, height: 20, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 1.8, strokeLinecap: "round" as const, strokeLinejoin: "round" as const, "aria-hidden": true };

/** Polls `get` every `ms` and keeps the latest value. */
function usePolled<T>(get: () => Promise<T>, initial: T, ms: number): [T, (v: T) => void] {
  const [v, setV] = useState<T>(initial);
  useEffect(() => {
    const load = () => void get().then(setV);
    load();
    const t = setInterval(load, ms);
    return () => clearInterval(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return [v, setV];
}

/** Mute switch of the default microphone. */
export function Microphone() {
  const [m, setM] = usePolled<MicState>(micGet, { available: true, muted: false }, 1500);
  return (
    <div
      className={m.muted ? "tile notif mic muted" : "tile notif mic"}
      title={!m.available ? "Mikrofon bulunamadı" : m.muted ? "Mikrofon kapalı · açmak için tıkla" : "Mikrofon açık · kapatmak için tıkla"}
      onClick={() => void micToggle().then(setM)}
    >
      <svg {...svg}>
        <rect x="9" y="3" width="6" height="11" rx="3" />
        <path d="M5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21M9 21h6" />
        {m.muted && <path d="M4 4l16 16" strokeWidth="2.2" />}
      </svg>
    </div>
  );
}

const items = (n: number) => (n > 99 ? "99+" : String(n));

/** Recycle Bin: item count, click opens it, right-click empties it (Windows asks first). */
export function RecycleBin() {
  const [b] = usePolled<BinState>(binGet, { items: 0, bytes: 0 }, 3000);
  return (
    <div
      className="tile notif bin"
      title={b.items ? `${b.items} öğe · açmak için tıkla, boşaltmak için sağ tıkla` : "Geri Dönüşüm Kutusu boş"}
      onClick={() => void binOpen()}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        if (b.items) void binEmpty();
      }}
    >
      <svg {...svg}>
        <path d="M4 7h16M10 11v6M14 11v6M6 7l1 12.2A2 2 0 0 0 9 21h6a2 2 0 0 0 2-1.8L18 7M9 7V4.5A1.5 1.5 0 0 1 10.5 3h3A1.5 1.5 0 0 1 15 4.5V7" />
      </svg>
      {b.items > 0 && <b className="bin-n">{items(b.items)}</b>}
    </div>
  );
}

/** Puts a new sticky note on the desktop; the count shows how many there are. */
export function StickyNotes() {
  const [n] = usePolled<number>(notesCount, 0, 2000);
  return (
    <div className="tile notif sticky" title="Yeni yapışkan not" onClick={() => void noteNew()}>
      <svg {...svg}>
        <path d="M5 4h14v10l-5 6H5zM14 20v-5a1 1 0 0 1 1-1h4M8.5 9h7M8.5 12.5h4" />
      </svg>
      {n > 0 && <b className="bin-n">{n}</b>}
    </div>
  );
}

/** Clipboard history: click opens the list of recent copies above the dock. */
export function Clipboard() {
  const [n] = usePolled<number>(clipCount, 0, 1500);
  const ref = useRef<HTMLDivElement>(null);
  const popOpen = useEvent<boolean>(EVENTS.langOpen, false);
  const openAtDown = useRef(false);
  return (
    <div
      ref={ref}
      className="tile notif clip"
      title={n ? `${n} kayıt · listeyi aç` : "Pano geçmişi · henüz boş"}
      onPointerDown={() => {
        openAtDown.current = popOpen;
      }}
      onClick={() => {
        const r = ref.current!.getBoundingClientRect();
        void appMenuToggle(r.left + r.width / 2, "clip", openAtDown.current);
      }}
    >
      <svg {...svg}>
        <path d="M9 4h6v3H9zM8 5.5H6.5A1.5 1.5 0 0 0 5 7v12a1.5 1.5 0 0 0 1.5 1.5h11A1.5 1.5 0 0 0 19 19V7a1.5 1.5 0 0 0-1.5-1.5H16M9 12h6M9 16h4" />
      </svg>
      {n > 0 && <b className="bin-n">{n > 99 ? "99+" : n}</b>}
    </div>
  );
}
