import { useState } from "react";
import { EVENTS, mediaControl, useEvent, type MediaState } from "../lib/ipc";

// shortcut: inline style until Task 14 adds the stylesheet; move to CSS classes then.
const ellipsis = { maxWidth: 140, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", display: "block" } as const;

export function NowPlaying() {
  const m = useEvent<MediaState | null>(EVENTS.media, null);
  const [hover, setHover] = useState(false);
  if (!m) return null;
  return (
    <div className="tile nowplaying" onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)} title={`${m.title} · ${m.artist}`}>
      {m.artDataUrl ? <img className="art" src={m.artDataUrl} alt="" /> : <div className="art" />}
      <div className="np-text" style={ellipsis}>
        <span className="np-title" style={ellipsis}>{m.title}</span>
        <span className="dim np-artist" style={ellipsis}>{m.artist}</span>
      </div>
      {hover && (
      <div className="np-controls">
        <button aria-label="Önceki" onClick={() => mediaControl("prev")}>⏮</button>
        <button aria-label={m.playing ? "Duraklat" : "Oynat"} onClick={() => mediaControl("toggle")}>
          {m.playing ? "⏸" : "▶"}
        </button>
        <button aria-label="Sonraki" onClick={() => mediaControl("next")}>⏭</button>
      </div>
      )}
    </div>
  );
}
