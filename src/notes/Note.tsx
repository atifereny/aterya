import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { noteDelete, noteFlags, noteGet, noteSet, type NoteColor } from "../lib/ipc";

const COLORS: NoteColor[] = ["sun", "ember", "teal", "rose", "lilac"];

/** One sticky note window: a colored card with a drag header, free text and a resize grip. */
export function Note() {
  const id = location.hash.replace("#note=", "");
  const [text, setText] = useState("");
  const [color, setColor] = useState<NoteColor>("sun");
  const [ready, setReady] = useState(false);
  const [sure, setSure] = useState(false);
  const [locked, setLocked] = useState(false);
  const [top, setTop] = useState(false);
  const timer = useRef(0);
  useEffect(() => {
    void noteGet(id).then((n) => {
      if (n) {
        setText(n.text);
        setColor(n.color);
        setLocked(n.locked);
        setTop(n.top);
      }
      setReady(true);
    });
  }, [id]);
  const save = (t: string, c: NoteColor) => {
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => void noteSet(id, t, c), 350);
  };
  const w = getCurrentWindow();
  return (
    <div className={`snote note-${color}`}>
      <div className="note-head" data-tauri-drag-region={locked ? undefined : ""}>
        <div className="note-dots">
          {COLORS.map((c) => (
            <button
              key={c}
              className={`note-dot note-${c}${c === color ? " sel" : ""}`}
              aria-label={`Renk: ${c}`}
              onClick={() => {
                setColor(c);
                save(text, c);
              }}
            />
          ))}
        </div>
        <div className="note-tools">
          <button
            className={locked ? "note-btn on" : "note-btn"}
            title={locked ? "Konum sabit · çözmek için tıkla" : "Bu konumda sabitle"}
            aria-pressed={locked}
            onClick={() => {
              setLocked(!locked);
              void noteFlags(id, !locked, top);
            }}
          >
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
              <rect x="5" y="11" width="14" height="9" rx="2.5" fill={locked ? "currentColor" : "none"} />
              <path d={locked ? "M8 11V8a4 4 0 0 1 8 0v3" : "M8 11V8a4 4 0 0 1 7.6-1.7"} />
            </svg>
          </button>
          <button
            className={top ? "note-btn on" : "note-btn"}
            title={top ? "Her şeyin üstünde · kapatmak için tıkla" : "Her şeyin üstünde tut"}
            aria-pressed={top}
            onClick={() => {
              setTop(!top);
              void noteFlags(id, locked, !top);
            }}
          >
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
              <path d="M5 4h14M12 21V9M7.5 13.5 12 9l4.5 4.5" />
            </svg>
          </button>
        <button
          className={sure ? "note-del sure" : "note-del"}
          title={sure ? "Silmek için bir daha tıkla" : "Notu sil"}
          onBlur={() => setSure(false)}
          onClick={() => {
            if (!text.trim() || sure) void noteDelete(id);
            else setSure(true);
          }}
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
            <path d="M4 7h16M10 11v6M14 11v6M6 7l1 12.2A2 2 0 0 0 9 21h6a2 2 0 0 0 2-1.8L18 7M9 7V4.5A1.5 1.5 0 0 1 10.5 3h3A1.5 1.5 0 0 1 15 4.5V7" />
          </svg>
          {sure && <span>Sil?</span>}
        </button>
        </div>
      </div>
      <textarea
        value={text}
        disabled={!ready}
        placeholder="Notunu yaz…"
        spellCheck={false}
        onChange={(e) => {
          setText(e.target.value);
          save(e.target.value, color);
        }}
        autoFocus
      />
      {!locked && <div className="note-grip" onPointerDown={() => void w.startResizeDragging("SouthEast")} />}
    </div>
  );
}
