import { useEffect, useMemo, useRef, useState } from "react";
import { calEventAdd, calEventDelete, calEventUpdate, calFit, calRefresh, EVENTS, useEvent, type CalEvent } from "../lib/ipc";
import { CAL_COLORS } from "./colors";
import { useCalendar } from "../widgets/Calendar";
import { addDays, dayTitle, hhmm, monthGrid, monthName, onDay, sameDay, startOfDay, WEEKDAYS } from "./dates";

function Row({ e, onDeleted, onEdit }: { e: CalEvent; onDeleted: () => void; onEdit: () => void }) {
  return (
    <div className="cal-ev" style={{ borderLeftColor: e.color }}>
      <div className="cal-ev-t">
        <span className="cal-ev-time">{e.allDay ? "Tüm gün" : `${hhmm(e.start)} – ${hhmm(e.end)}`}</span>
        <b>{e.title || "(Adsız)"}</b>
        {e.location && <span className="dim cal-ev-loc">{e.location}</span>}
      </div>
      <span className="dim cal-ev-src">{e.source}</span>
      {e.local && (
        <button className="cal-del" aria-label="Etkinliği düzenle" title="Düzenle" onClick={onEdit}>
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.1" strokeLinecap="round" strokeLinejoin="round" aria-hidden><path d="M4 20h4L19 9l-4-4L4 16v4zM13.5 6.5l4 4" /></svg>
        </button>
      )}
      {e.local && (
        <button className="cal-del" aria-label="Etkinliği sil" title="Sil" onClick={() => void calEventDelete(e.id).then(onDeleted)}>
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" aria-hidden><path d="M6 6l12 12M18 6L6 18" /></svg>
        </button>
      )}
    </div>
  );
}

/** Add an event on `day`, or change `edit` (one of the user's own events). */
function EventForm({ day, edit, onDone, onSaved }: { day: Date; edit?: CalEvent; onDone: () => void; onSaved: () => void }) {
  const [title, setTitle] = useState(edit?.title ?? "");
  const [allDay, setAllDay] = useState(edit?.allDay ?? false);
  const nextHour = (new Date().getHours() + 1) % 24;
  const two = (n: number) => String(n).padStart(2, "0");
  const [from, setFrom] = useState(edit && !edit.allDay ? hhmm(edit.start) : `${two(nextHour)}:00`);
  const [to, setTo] = useState(edit && !edit.allDay ? hhmm(edit.end) : `${two((nextHour + 1) % 24)}:00`);
  const [color, setColor] = useState(edit?.color || CAL_COLORS[5]);
  const [err, setErr] = useState("");
  const base = edit ? startOfDay(new Date(edit.start)) : day;
  const at = (hm: string) => {
    const [h, m] = hm.split(":").map(Number);
    return new Date(base.getFullYear(), base.getMonth(), base.getDate(), h, m).getTime();
  };
  const save = () => {
    const d0 = startOfDay(base).getTime();
    const [s, e] = allDay ? [d0, addDays(base, 1).getTime()] : [at(from), Math.max(at(to), at(from))];
    (edit ? calEventUpdate(edit.id, title, s, e, allDay, color) : calEventAdd(title, s, e, allDay, color))
      .then(() => {
        onSaved();
        onDone();
      })
      .catch((x) => setErr(String(x)));
  };
  return (
    <div className="cal-add">
      <input className="cal-in" autoFocus placeholder="Etkinlik adı" value={title} maxLength={120} onChange={(e) => setTitle(e.target.value)} onKeyDown={(e) => e.key === "Enter" && save()} />
      <div className="cal-add-row">
        <label className="cal-chk"><input type="checkbox" checked={allDay} onChange={(e) => setAllDay(e.target.checked)} /> Tüm gün</label>
        {!allDay && (
          <>
            <input className="cal-in cal-time" type="time" value={from} onChange={(e) => setFrom(e.target.value)} />
            <span className="dim">–</span>
            <input className="cal-in cal-time" type="time" value={to} onChange={(e) => setTo(e.target.value)} />
          </>
        )}
      </div>
      <div className="cal-add-row">
        {CAL_COLORS.map((c) => (
          <button key={c} className={c === color ? "cal-sw on" : "cal-sw"} style={{ background: c }} onClick={() => setColor(c)} aria-label={c} />
        ))}
        <span className="grow" />
        <button className="cal-btn" onClick={onDone}>Vazgeç</button>
        <button className="cal-btn pri" onClick={save}>{edit ? "Kaydet" : "Ekle"}</button>
      </div>
      {err && <div className="cal-err">{err}</div>}
    </div>
  );
}

export function CalPopup() {
  const openEvt = useEvent<boolean>(EVENTS.calOpen, false);
  const [open, setOpen] = useState(false);
  useEffect(() => {
    if (!openEvt) return setOpen(false);
    const a = requestAnimationFrame(() => requestAnimationFrame(() => setOpen(true)));
    return () => cancelAnimationFrame(a);
  }, [openEvt]);
  const [fallback, setFallback] = useState(false);
  useEffect(() => {
    const t = setTimeout(() => setFallback(true), 400);
    return () => clearTimeout(t);
  }, []);
  const seenEvent = useRef(false);
  if (openEvt) seenEvent.current = true;

  const [month, setMonth] = useState(() => startOfDay(new Date()));
  const [sel, setSel] = useState(() => startOfDay(new Date()));
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<CalEvent | null>(null);
  const [spin, setSpin] = useState(false);
  const [bump, setBump] = useState(0); // reload right after this window changes an event itself
  // Back to today each time it opens.
  useEffect(() => {
    if (openEvt) {
      const t = startOfDay(new Date());
      setMonth(t);
      setSel(t);
      setAdding(false);
      setEditing(null);
    }
  }, [openEvt]);

  const grid = useMemo(() => monthGrid(month.getFullYear(), month.getMonth()), [month]);
  const { sources, events } = useCalendar(grid[0].getTime(), addDays(grid[41], 1).getTime(), bump * 2 + (openEvt ? 1 : 0));
  const today = startOfDay(new Date());
  const dayEvents = events.filter((e) => onDay(e, sel));

  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const ro = new ResizeObserver(() => void calFit(el.offsetHeight + 30));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const go = (n: number) => setMonth((m) => new Date(m.getFullYear(), m.getMonth() + n, 1));
  const refresh = () => {
    setSpin(true);
    void calRefresh().finally(() => setSpin(false));
  };
  return (
    <div className={open || (fallback && !seenEvent.current) ? "mx cal in" : "mx cal"}>
      <div ref={box} className="cal-box">
        <div className="cal-head">
          <b className="cal-month">{monthName(month)}</b>
          <span className="grow" />
          <button className="cal-ic" onClick={() => go(-1)} aria-label="Önceki ay">‹</button>
          <button className="cal-btn" onClick={() => { setMonth(today); setSel(today); }}>Bugün</button>
          <button className="cal-ic" onClick={() => go(1)} aria-label="Sonraki ay">›</button>
          <button className={spin ? "cal-ic spin" : "cal-ic"} onClick={refresh} aria-label="Takvimleri yenile" title="Takvimleri yenile">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden><path d="M20 11a8 8 0 10-2.3 5.7M20 4v7h-7" /></svg>
          </button>
        </div>
        <div className="cal-grid cal-wd">
          {WEEKDAYS.map((w) => <span key={w}>{w}</span>)}
        </div>
        <div className="cal-grid">
          {grid.map((d) => {
            const evs = events.filter((e) => onDay(e, d));
            const colors = [...new Set(evs.map((e) => e.color))].slice(0, 3);
            return (
              <button key={d.getTime()} className={`cal-day${d.getMonth() !== month.getMonth() ? " other" : ""}${sameDay(d, today) ? " today" : ""}${sameDay(d, sel) ? " sel" : ""}`} onClick={() => { setSel(d); setAdding(false); setEditing(null); }}>
                <span>{d.getDate()}</span>
                <i className="cal-dots">{colors.map((c) => <i key={c} style={{ background: c }} />)}</i>
              </button>
            );
          })}
        </div>
        <div className="cal-sep" />
        <div className="cal-agenda-h">
          <b>{dayTitle(sel)}</b>
          <span className="grow" />
          {!adding && !editing && <button className="cal-btn" onClick={() => setAdding(true)}>+ Etkinlik</button>}
        </div>
        {adding && <EventForm day={sel} onDone={() => setAdding(false)} onSaved={() => setBump((b) => b + 1)} />}
        {editing && <EventForm key={editing.id} day={sel} edit={editing} onDone={() => setEditing(null)} onSaved={() => setBump((b) => b + 1)} />}
        <div className="cal-list">
          {dayEvents.length === 0 && !adding && !editing && <div className="dim cal-empty">Bu gün için etkinlik yok</div>}
          {dayEvents.map((e) => <Row key={e.id} e={e} onDeleted={() => setBump((b) => b + 1)} onEdit={() => { setAdding(false); setEditing(e); }} />)}
        </div>
        {sources.length === 0 && <div className="dim cal-hint">Google Takvim gibi bir takvim eklemek için: Ayarlar &gt; Takvim</div>}
        {sources.some((s) => s.error) && <div className="cal-err">{sources.filter((s) => s.error).map((s) => `${s.name}: ${s.error}`).join(" · ")}</div>}
      </div>
    </div>
  );
}
