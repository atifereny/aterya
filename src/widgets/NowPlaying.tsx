import { EVENTS, mediaControl, useEvent, type MediaState } from "../lib/ipc";

const ellipsis = { maxWidth: 140, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", display: "block" } as const;

export function NowPlaying() {
  const m = useEvent<MediaState | null>(EVENTS.media, null);
  if (!m) return null;
  return (
    <div className="tile nowplaying" title={m.artist ? `${m.title} · ${m.artist}` : m.title}>
      {m.artDataUrl ? <img className="art" src={m.artDataUrl} alt="" /> : <div className="art" />}
      <div className="np-text" style={ellipsis}>
        <span className="np-title" style={ellipsis}>{m.title}</span>
        <span className="dim np-artist" style={ellipsis}>{m.artist}</span>
      </div>
      <div className="np-controls">
        <button aria-label="Önceki" onClick={() => mediaControl("prev")}>⏮</button>
        <button aria-label={m.playing ? "Duraklat" : "Oynat"} onClick={() => mediaControl("toggle")}>
          {m.playing ? "⏸" : "▶"}
        </button>
        <button aria-label="Sonraki" onClick={() => mediaControl("next")}>⏭</button>
      </div>
    </div>
  );
}
