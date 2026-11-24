import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { calRefresh, calSourceAdd, calSourceRemove, calSourceUpdate, calState, type CalSourceInfo } from "../lib/ipc";
import { CAL_COLORS as COLORS } from "../cal/colors";

/** One calendar: its name and color, with an edit mode for both. */
function SourceCard({ c }: { c: CalSourceInfo }) {
  const [edit, setEdit] = useState(false);
  const [name, setName] = useState(c.name);
  const [color, setColor] = useState(c.color);
  const [err, setErr] = useState("");
  const open = () => {
    setName(c.name);
    setColor(c.color);
    setErr("");
    setEdit(true);
  };
  const save = () => {
    calSourceUpdate(c.id, name, color)
      .then(() => setEdit(false))
      .catch((e) => setErr(String(e)));
  };
  if (!edit)
    return (
      <div className="card static">
        <i className="cal-swatch" style={{ background: c.color }} />
        <div className="meta">
          <b>{c.name}</b>
          <span>{c.error ? c.error : `${c.count} etkinlik`}</span>
        </div>
        <button className="keybtn" onClick={open}>Düzenle</button>
        <button className="keybtn" onClick={() => void calSourceRemove(c.id)}>Kaldır</button>
      </div>
    );
  return (
    <div className="card static cal-form">
      <input className="cal-field" value={name} maxLength={60} autoFocus onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && save()} />
      <div className="cal-colors">
        {COLORS.map((x) => (
          <button key={x} className={x === color ? "cal-sw on" : "cal-sw"} style={{ background: x }} onClick={() => setColor(x)} aria-label={x} />
        ))}
        <input className="cal-pick" type="color" value={color} onChange={(e) => setColor(e.target.value)} aria-label="Özel renk" title="Özel renk" />
        <span className="grow" />
        <button className="keybtn" onClick={() => setEdit(false)}>Vazgeç</button>
        <button className="keybtn rec" disabled={!name.trim()} onClick={save}>Kaydet</button>
      </div>
      {err && <div className="cal-ferr">{err}</div>}
    </div>
  );
}

/** Calendars the dock shows: iCal (ICS) addresses from Google Calendar, Outlook, iCloud, ... */
export function CalendarTab() {
  const [sources, setSources] = useState<CalSourceInfo[]>([]);
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [color, setColor] = useState(COLORS[0]);
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let live = true;
    const load = () => void calState(0, 1).then((c) => live && setSources(c.sources));
    load();
    const un = listen("cal://changed", load);
    return () => {
      live = false;
      void un.then((f) => f());
    };
  }, []);
  const add = async () => {
    setErr("");
    setBusy(true);
    try {
      await calSourceAdd(name, url, color);
      setName("");
      setUrl("");
      setColor(COLORS[(sources.length + 1) % COLORS.length]);
    } catch (e) {
      setErr(String(e));
    }
    setBusy(false);
  };
  return (
    <>
      <h2>Takvim</h2>
      <p className="sub">Takvim widget'ı bu takvimlerdeki etkinlikleri gösterir. Takvimler yalnızca okunur; kendi etkinliklerini widget'tan ekleyebilirsin.</p>
      {sources.length === 0 && (
        <div className="card static">
          <div className="meta">
            <b>Henüz takvim eklenmedi</b>
            <span>Aşağıdan bir takvimin iCal adresini ekle.</span>
          </div>
        </div>
      )}
      {sources.map((c) => (
        <SourceCard key={c.id} c={c} />
      ))}
      <div className="card static">
        <button className="keybtn" onClick={() => void calRefresh()}>Şimdi yenile</button>
        <div className="meta"><span>Takvimler otomatik olarak 15 dakikada bir yenilenir.</span></div>
      </div>
      <h3 className="sub-h">Takvim ekle</h3>
      <div className="card static cal-form">
        <input className="cal-field" placeholder="Takvimin adı (örn. Kişisel)" value={name} onChange={(e) => setName(e.target.value)} />
        <input className="cal-field" placeholder="iCal adresi (https://… veya webcal://…)" value={url} spellCheck={false} onChange={(e) => setUrl(e.target.value)} />
        <div className="cal-colors">
          {COLORS.map((c) => (
            <button key={c} className={c === color ? "cal-sw on" : "cal-sw"} style={{ background: c }} onClick={() => setColor(c)} aria-label={c} />
          ))}
          <input className="cal-pick" type="color" value={color} onChange={(e) => setColor(e.target.value)} aria-label="Özel renk" title="Özel renk" />
          <span className="grow" />
          <button className="keybtn rec" disabled={busy || !name.trim() || !url.trim()} onClick={() => void add()}>{busy ? "Ekleniyor…" : "Ekle"}</button>
        </div>
        {err && <div className="cal-ferr">{err}</div>}
      </div>
      <p className="note">
        Google Takvim: calendar.google.com &gt; takvimin yanındaki ⋮ &gt; Ayarlar ve paylaşım &gt; "Takvimi entegre et" &gt; <b>iCal biçimindeki gizli adres</b>'i kopyala.
        Outlook ve iCloud'da da takvimi "yayınla / herkese açık bağlantı" ile paylaşıp .ics adresini al. Bu adres bir parola gibidir: yalnızca bu bilgisayardaki ayar dosyanda saklanır, kimseyle paylaşma.
      </p>
    </>
  );
}
