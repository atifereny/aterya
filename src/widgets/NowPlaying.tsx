import { EVENTS, mediaControl, useEvent, type MediaState } from "../lib/ipc";

export function NowPlaying() {
  const m = useEvent<MediaState | null>(EVENTS.media, null);
  if (!m) return null;
  return (
    <div className="tile nowplaying" title={`${m.title} · ${m.artist}`}>
      {m.artDataUrl ? <img className="art" src={m.artDataUrl} alt="" /> : <div className="art" />}
      <div className="np-text">
        <span className="np-title">{m.title}</span>
        <span className="dim np-artist">{m.artist}</span>
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
