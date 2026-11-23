import { Fragment, useEffect, useLayoutEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { StartIcon, START_ICONS } from "../lib/StartIcon";
import { autostartGet, autostartSet, backupExport, backupImport, setAutoUpdate, updateCheck, updateInstall, updateState, type UpdateInfo, setAutohide, setDockEdge, setHoverLift, setDividerWidth, setIcons, setLook, demo, monitorsList, setMonitorOrder, setDock, type MonitorInfo, requestState, setIndicator, setStartIcon, floatClose, floatOpen, floatPin, quitApp, setBrightnessLayout, setWidgetScale, restartApp, setBringKey, setBringToMain, setLayout, setTheme, useSettings, type FloatCfg, type LayoutCfg, type Settings as Cfg, type ThemeCfg, type WidgetId } from "../lib/ipc";
import { WIDGET_INFO } from "../lib/widgets";
import { Zoom } from "../lib/Zoom";
import "./settings.css";

type Tab = "widgets" | "order" | "dock" | "desktop" | "design" | "behavior";

type Scheme = "system" | "dark" | "light";
// Accent: null follows the scheme (dark: orange, light: turquoise); the others are [dark value, light value].
const ACCENTS: { name: string; c: [string, string] }[] = [
  { name: "Turuncu", c: ["#ff7a1a", "#e8670c"] },
  { name: "Turkuaz", c: ["#0fb5ac", "#0fb5ac"] },
  { name: "Mavi", c: ["#5b8cff", "#3b6df0"] },
  { name: "Mor", c: ["#a78bfa", "#7c5cf0"] },
  { name: "Yeşil", c: ["#3fd07a", "#18a653"] },
];
type Look = { scheme: Scheme; accent: number | null };

function loadLook(): Look {
  try {
    const v = JSON.parse(localStorage.getItem("aterya-look") ?? "{}");
    return { scheme: ["dark", "light"].includes(v.scheme) ? v.scheme : "system", accent: typeof v.accent === "number" && ACCENTS[v.accent] ? v.accent : null };
  } catch {
    return { scheme: "system", accent: null };
  }
}

/** The settings window's own look: scheme and accent, kept in this window's storage and applied on <html>. */
function useLook(): [Look, (l: Look) => void] {
  const [look, setLook] = useState<Look>(loadLook);
  const [os, setOs] = useState(() => matchMedia("(prefers-color-scheme: dark)").matches);
  useEffect(() => {
    const m = matchMedia("(prefers-color-scheme: dark)");
    const f = () => setOs(m.matches);
    m.addEventListener("change", f);
    return () => m.removeEventListener("change", f);
  }, []);
  useLayoutEffect(() => {
    const dark = look.scheme === "system" ? os : look.scheme === "dark";
    const r = document.documentElement;
    r.dataset.scheme = dark ? "dark" : "light";
    const a = look.accent == null ? null : ACCENTS[look.accent].c[dark ? 0 : 1];
    for (const k of ["--acc", "--acc-text", "--acc-soft"]) r.style.removeProperty(k);
    if (a) {
      r.style.setProperty("--acc", a);
      r.style.setProperty("--acc-text", a);
      r.style.setProperty("--acc-soft", a + "29");
    }
  }, [look, os]);
  return [look, (l) => { setLook(l); try { localStorage.setItem("aterya-look", JSON.stringify(l)); } catch { /* storage can be unavailable */ } }];
}

const NAV_ICON: Record<Tab, string> = {
  widgets: "M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z",
  order: "M3 6h18v12H3zM8 6v12M16 6v12",
  dock: "M3 6h18v12H3zM3 14h18",
  desktop: "M3 5h18v11H3zM8 20h8M12 16v4",
  design: "M12 3a9 9 0 1 0 0 18c1.4 0 2-1 1.4-2.1-.6-1.2.2-2.4 1.6-2.4H17a4 4 0 0 0 4-4C21 6.6 17 3 12 3z",
  behavior: "M4 7h10M18 7h2M4 17h2M10 17h10M16 5v4M8 15v4",
};
const TAB_NAME: Record<Tab, string> = { widgets: "Widget'lar", order: "Çubuk", dock: "Dock", desktop: "Masaüstü", design: "Tasarım", behavior: "Davranış" };

const CATEGORIES: { name: string; desc: string }[] = [
  { name: "Uygulamalar ve dosyalar", desc: "Başlat, sabit uygulamalar, son açılanlar, arama, komutlar." },
  { name: "Hava durumu", desc: "Şehrin anlık durumu ve tahminleri." },
  { name: "Saatler", desc: "Her biçimde saat, tarih ve dünya saatleri." },
  { name: "Medya ve ses", desc: "Çalan müzik, ses çıkışı, mikrofon." },
  { name: "Sistem", desc: "Pil, ağ, işlemci, bellek ve hızlı anahtarlar." },
  { name: "Üretkenlik", desc: "Zamanlayıcılar, notlar, görevler, takvim." },
  { name: "Yapay zekâ", desc: "Yapay zekâ araçlarının kullanım bilgileri." },
];

const PRESETS: { id: ThemeCfg["preset"]; name: string; desc: string }[] = [
  { id: "glass", name: "Liquid Glass", desc: "Bulanık cam, duvar kâğıdının renkleri görünür." },
  { id: "dark", name: "Koyu", desc: "Daha opak, koyu bir bar." },
  { id: "light", name: "Açık", desc: "Açık renkli buzlu cam." },
];

function Switch({ on, onChange, label }: { on: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button role="switch" aria-checked={on} aria-label={label} className={on ? "sw on" : "sw"} onClick={() => onChange(!on)}>
      <i />
    </button>
  );
}

const APPS_PREVIEW = (
  <>
    <span className="sw-mini-app" style={{ background: "#4b8bff" }} />
    <span className="sw-mini-app" style={{ background: "#e0572f" }} />
    <span className="sw-mini-app" style={{ background: "#34d399" }} />
    <span className="sw-mini-app" style={{ background: "#fbbf24" }} />
  </>
);

/** The real widget, live: what you see here is what the dock shows. Clicks are off (see .pvlive). */
function Preview({ id, zoom = 1, scale, fit }: { id: WidgetId; zoom?: number; scale?: number; fit?: { w: number; h: number } }) {
  const ref = useRef<HTMLDivElement>(null);
  // A big widget size would spill out of its box: shrink the preview until it fits (set on the element directly, so it cannot loop).
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || !fit) return;
    const measure = () => {
      el.style.zoom = "1";
      const r = el.getBoundingClientRect();
      el.style.zoom = String(r.width && r.height ? Math.min(zoom, fit.w / r.width, fit.h / r.height) : zoom);
    };
    measure();
    const t = setInterval(measure, 700); // content (sensor values) changes size over time
    return () => clearInterval(t);
  }, [zoom, scale, fit?.w, fit?.h]);
  if (id === "apps") return <div className="sw-mini">{APPS_PREVIEW}</div>;
  return (
    <div className="pvlive" ref={ref} style={fit ? undefined : { zoom }}>
      <Zoom scale={scale}>{WIDGET_INFO[id].render()}</Zoom>
    </div>
  );
}

// Pointer-based drag: WebView2 in Tauri swallows HTML5 drag-and-drop, so it is done by hand.
function DividerWidth({ value }: { value: number }) {
  // The slider follows the pointer at once; the update goes out at most every 90 ms, so dragging never stalls the dock.
  const [w, setW] = useState(value);
  const t = useRef({ timer: 0, latest: value, busy: 0 });
  useEffect(() => {
    if (Date.now() - t.current.busy > 400) setW(value);
  }, [value]);
  return (
    <div className="slider" style={{ marginTop: 18 }}>
      <label htmlFor="dw" style={{ width: 130 }}>Çizgi kalınlığı</label>
      <input
        id="dw"
        type="range"
        min={1}
        max={6}
        step={0.5}
        value={w}
        onChange={(e) => {
          const v = Number(e.target.value);
          setW(v);
          const s = t.current;
          s.latest = v;
          s.busy = Date.now();
          if (s.timer) return;
          s.timer = window.setTimeout(() => {
            s.timer = 0;
            void setDividerWidth(s.latest);
          }, 90);
        }}
      />
      <span style={{ width: 44 }}>{w} px</span>
    </div>
  );
}

function Order({ layout, floating, layout_scales, dividerWidth }: { layout: LayoutCfg; floating: FloatCfg[]; layout_scales: Cfg["scales"]; dividerWidth: number }) {
  const [drag, setDrag] = useState<WidgetId | null>(null);
  const [live, setLive] = useState<WidgetId[] | null>(null);
  const shown = layout.order.filter((id) => !layout.hidden.includes(id) && !floating.some((f) => f.id === id));
  const order = live ?? shown;
  const over = (x: number, y: number) =>
    (document.elementFromPoint(x, y)?.closest("[data-wid]") as HTMLElement | null)?.dataset.wid as WidgetId | undefined;
  const end = () => {
    if (live) {
      // Hidden widgets keep their slots; the shown ones fill the rest in the new order.
      const q = [...live];
      // Floating widgets are not in the dock either, so they keep their slots too.
      const fixed = (id: WidgetId) => layout.hidden.includes(id) || floating.some((f) => f.id === id);
      void setLayout({ ...layout, order: layout.order.map((id) => (fixed(id) ? id : q.shift()!)) });
    }
    setDrag(null);
    setLive(null);
  };
  return (
    <>
      <h2>Çubuk</h2>
      <p className="sub">Dock'taki sıra soldan sağa böyle. Kutuyu sürükleyip bırakarak yerini değiştir. Kutuların arasındaki çubuğa tıklayarak o yere ayırma çizgisi koy.</p>
      <div
        className="strip"
        onPointerMove={(e) => {
          if (!drag) return;
          const to = over(e.clientX, e.clientY);
          if (!to || to === drag) return;
          const cur = live ?? shown;
          const next = cur.filter((w) => w !== drag);
          next.splice(cur.indexOf(to), 0, drag);
          setLive(next);
        }}
        onPointerUp={end}
        onPointerCancel={end}
      >
        {order.map((id, idx) => (
          <Fragment key={id}>
            <div
              data-wid={id}
              className={`tilecard${drag === id ? " dragging" : ""}`}
              onPointerDown={(e) => {
                e.preventDefault(); // no text selection / native drag starting on the dots, which would cancel the pointer
                e.currentTarget.parentElement!.setPointerCapture(e.pointerId);
                setDrag(id);
              }}
            >
              <span className="grip" aria-hidden>
                ⋮⋮
              </span>
              <Preview id={id} zoom={0.55} scale={layout_scales[id]} />
              <b>{WIDGET_INFO[id].label}</b>
            </div>
            {idx < order.length - 1 && (
              <button
                className={layout.dividers.includes(id) ? "divbtn on" : "divbtn"}
                title={layout.dividers.includes(id) ? "Çizgiyi kaldır" : "Buraya çizgi koy"}
                aria-label={layout.dividers.includes(id) ? "Çizgiyi kaldır" : "Buraya çizgi koy"}
                aria-pressed={layout.dividers.includes(id)}
                onPointerDown={(e) => e.stopPropagation()}
                onClick={() => void setLayout({ ...layout, dividers: layout.dividers.includes(id) ? layout.dividers.filter((d) => d !== id) : [...layout.dividers, id] })}
              >
                <i />
              </button>
            )}
          </Fragment>
        ))}
        {!order.length && <span className="note">Dock'ta widget yok. Widget'lar sekmesinden ekle.</span>}
      </div>
      <DividerWidth value={dividerWidth} />
      <p className="note">Widget eklemek veya çıkarmak için Widget'lar sekmesini kullan. Başlat düğmesi ve uygulama simgeleri her zaman solda sabit.</p>
    </>
  );
}

function Catalog({ layout, floating, goDesktop, scales, brightnessLayout }: { layout: LayoutCfg; floating: FloatCfg[]; goDesktop: () => void; scales: Cfg["scales"]; brightnessLayout: Cfg["brightnessLayout"] }) {
  // On means "somewhere on screen": a newly enabled widget goes to the desktop, and "Çubuğa ekle" moves it into the dock.
  const toggle = async (id: WidgetId, on: boolean) => {
    const floats = floating.some((f) => f.id === id);
    if (on && id !== "apps" && !floats) await floatOpen(id);
    if (!on && floats) await floatClose(id);
    await setLayout({ ...layout, hidden: on ? layout.hidden.filter((h) => h !== id) : [...layout.hidden, id] });
  };
  const [q, setQ] = useState("");
  const [cat, setCat] = useState("Tümü");
  const needle = q.trim().toLocaleLowerCase("tr");
  const ids = (Object.keys(WIDGET_INFO) as WidgetId[]).filter((id) => !needle || `${WIDGET_INFO[id].label} ${WIDGET_INFO[id].desc}`.toLocaleLowerCase("tr").includes(needle));
  return (
    <>
      <h2>Widget'lar</h2>
      <p className="sub">Açtığın widget masaüstüne gelir. Çubukta görmek istediğine "Çubuğa ekle"ye bas.</p>
      <div className="toolbar">
        <label className="search">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden>
            <path d="M21 21l-4.3-4.3M10.5 18a7.5 7.5 0 1 1 0-15 7.5 7.5 0 0 1 0 15z" />
          </svg>
          <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Widget ara" aria-label="Widget ara" />
        </label>
      </div>
      <div className="cats">
        {["Tümü", ...CATEGORIES.map((c) => c.name)].map((c) => (
          <button key={c} className={cat === c ? "on" : ""} onClick={() => setCat(c)}>
            {c}
          </button>
        ))}
      </div>
      {CATEGORIES.filter((c) => cat === "Tümü" || c.name === cat).map((c) => {
        const items = ids.filter((id) => WIDGET_INFO[id].category === c.name);
        if (needle && !items.length) return null;
        return (
          <section key={c.name} className={items.length ? "cat" : "cat empty"}>
            <h3>
              {c.name} <em>{items.length ? items.length : "yakında"}</em>
            </h3>
            <p className="sub">{c.desc}</p>
            {items.map((id) => {
              const w = WIDGET_INFO[id];
              const visible = !layout.hidden.includes(id);
              return (
                <div key={id} className={visible ? "card static" : "card static off"}>
                  <div className="pvbox">
                    <Preview id={id} zoom={0.85} scale={scales[id]} fit={{ w: 252, h: 84 }} />
                  </div>
                  <div className="meta">
                    <b>{w.label}</b>
                    <span>{w.desc}</span>
                    {id !== "apps" && (
                      <div className="sizer">
                        <span>Boyut</span>
                        <input type="range" min={60} max={300} step={5} value={Math.round((scales[id] ?? 1) * 100)} onChange={(e) => void setWidgetScale(id, Number(e.target.value) / 100)} aria-label={`${w.label} boyutu`} />
                        <b>{Math.round((scales[id] ?? 1) * 100)}%</b>
                        {(scales[id] ?? 1) !== 1 && (
                          <button className="keybtn" onClick={() => void setWidgetScale(id, 1)}>
                            Sıfırla
                          </button>
                        )}
                      </div>
                    )}
                    {id === "brightness" && (
                      <div className="sizer">
                        <span>Düzen</span>
                        <button className={brightnessLayout === "column" ? "keybtn" : "keybtn rec"} onClick={() => void setBrightnessLayout("row")}>
                          Yan yana
                        </button>
                        <button className={brightnessLayout === "column" ? "keybtn rec" : "keybtn"} onClick={() => void setBrightnessLayout("column")}>
                          Alt alta
                        </button>
                      </div>
                    )}
                  </div>
                  <div className="act">
                    <div className="r1">
                    <span className={floating.some((f) => f.id === id) ? "chip acc" : "chip"}>{!visible ? "Kapalı" : floating.some((f) => f.id === id) ? "Masaüstünde" : "Çubukta"}</span>
  <Switch on={visible} onChange={(v) => void toggle(id, v)} label={`${w.label} göster`} />
                    </div>
                    <div className="r2">
                  {id !== "apps" &&
                    (floating.some((f) => f.id === id) ? (
                      <>
                        <button className="keybtn" onClick={goDesktop}>
                          Masaüstünde
                        </button>
                        <button className="keybtn rec" title="Masaüstünden alıp çubuğa koy" onClick={() => void floatClose(id)}>
                          Çubuğa ekle
                        </button>
                      </>
                    ) : (
                      visible && (
                        <button className="keybtn" title="Dock'tan çıkar, ekranın ortasına getir" onClick={() => void floatOpen(id)}>
                          Masaüstüne al
                        </button>
                      )
                    ))}
                    </div>
                  </div>
                </div>
              );
            })}
          </section>
        );
      })}
    </>
  );
}

/** Two sliders (size and gap of the app icons). They follow the pointer at once; the change goes out at most every 90 ms. */
function IconSliders({ size, gap }: { size: number; gap: number }) {
  const [sz, setSz] = useState(size);
  const [gp, setGp] = useState(gap);
  const t = useRef({ timer: 0, size, gap, busy: 0 });
  useEffect(() => {
    if (Date.now() - t.current.busy > 400) {
      setSz(size);
      setGp(gap);
    }
  }, [size, gap]);
  const send = (p: { size?: number; gap?: number }) => {
    const s = t.current;
    Object.assign(s, p);
    s.busy = Date.now();
    if (s.timer) return;
    s.timer = window.setTimeout(() => {
      s.timer = 0;
      void setIcons(s.size, s.gap);
    }, 90);
  };
  return (
    <>
      <div className="slider">
        <label htmlFor="isz" style={{ width: 150 }}>Simge boyutu</label>
        <input id="isz" type="range" min={20} max={44} step={1} value={sz} onChange={(e) => { const v = Number(e.target.value); setSz(v); send({ size: v }); }} />
        <span style={{ width: 44 }}>{sz} px</span>
      </div>
      <div className="slider">
        <label htmlFor="igp" style={{ width: 150 }}>Simgeler arası boşluk</label>
        <input id="igp" type="range" min={0} max={16} step={1} value={gp} onChange={(e) => { const v = Number(e.target.value); setGp(v); send({ gap: v }); }} />
        <span style={{ width: 44 }}>{gp} px</span>
      </div>
    </>
  );
}

/** Dock-wide settings: start with Windows, which screen, size and edge gap. */
function DockTab({ s }: { s: Cfg }) {
  const [auto, setAuto] = useState<boolean | null>(null);
  const [autoErr, setAutoErr] = useState("");
  const [mons, setMons] = useState<MonitorInfo[]>([]);
  useEffect(() => {
    void autostartGet().then(setAuto);
    void monitorsList().then(setMons);
  }, []);
  const chosen = s.dockMonitor && mons.some((m) => m.device === s.dockMonitor) ? s.dockMonitor : mons.find((m) => m.primary)?.device ?? "";
  // Sliders follow the pointer at once; the change goes to the dock at most every 80 ms.
  const [scale, setScale] = useState(Math.round(s.dockScale * 100));
  const [gap, setGap] = useState(Math.round(s.dockGap));
  const timer = useRef(0);
  const latest = useRef({ monitor: s.dockMonitor, scale: s.dockScale, gap: s.dockGap });
  const send = (p: Partial<typeof latest.current>) => {
    latest.current = { ...latest.current, ...p };
    if (timer.current) return;
    timer.current = window.setTimeout(() => {
      timer.current = 0;
      const l = latest.current;
      void setDock(l.monitor, l.scale, l.gap);
    }, 80);
  };
  return (
    <>
      <h2>Dock</h2>
      <p className="sub">Dock'un nerede durduğu, ne kadar büyük olduğu ve ne zaman açıldığı.</p>
      <div className="card static">
        <div className="meta">
          <b>Windows açılınca başlat</b>
          <span>{autoErr || "Bilgisayarı açınca dock kendiliğinden gelir."}</span>
        </div>
        <Switch
          on={!!auto}
          label="Windows açılınca başlat"
          onChange={(v) => {
            setAutoErr("");
            setAuto(v);
            autostartSet(v).catch((e) => {
              setAuto(!v);
              setAutoErr(String(e));
            });
          }}
        />
      </div>
      <div className="card static">
        <div className="meta">
          <b>Otomatik gizle</b>
          <span>Dock ekranın altına kayıp saklanır. Fareyi ekranın alt kenarına götürünce ya da Windows tuşuna basınca geri gelir. Gizliyken pencereler tüm ekranı kullanır.</span>
        </div>
        <Switch on={s.dockAutohide} label="Otomatik gizle" onChange={(v) => void setAutohide(v)} />
      </div>
      <div className="card static">
        <div className="meta">
          <b>Konum</b>
          <span>Dock'un ekranın hangi kenarında duracağı. Sol ve sağda dock dikey olur ve yalnızca uygulamaları ile küçük widget'ları gösterir.</span>
        </div>
        <div className="keypick">
          {(["bottom", "top", "left", "right"] as const).map((k) => (
            <button key={k} className={s.dockEdge === k ? "keybtn rec" : "keybtn"} onClick={() => void setDockEdge(k)}>
              {k === "bottom" ? "Alt" : k === "top" ? "Üst" : k === "left" ? "Sol" : "Sağ"}
            </button>
          ))}
        </div>
      </div>
      <h3 className="sub-h">Ekran</h3>
      <div className="monpick">
        {mons.map((m) => (
          <div className="monwrap" key={m.device}>
          <button
            className={chosen === m.device ? "mon sel" : "mon"}
            onClick={() => {
              latest.current = { ...latest.current, monitor: m.primary ? "" : m.device };
              send({});
            }}
          >
            <span className="mon-box" style={{ width: Math.round(m.width * Math.min(100 / m.width, 64 / m.height)), height: Math.round(m.height * Math.min(100 / m.width, 64 / m.height)) }}>
              {m.num}
            </span>
            <b>{m.primary ? "Ana ekran" : `Ekran ${m.num}`}</b>
            <span>
              {m.width} × {m.height}
            </span>
          </button>
          {mons.length > 1 && (
            <div className="monnums" role="group" aria-label="Ekran numarası">
              {mons.map((_, k) => (
                <button
                  key={k}
                  className={m.num === k + 1 ? "on" : ""}
                  onClick={() => {
                    // The screen holding that number takes this one's old number.
                    const order = [...mons].sort((a, b) => a.num - b.num).map((x) => x.device);
                    const from = m.num - 1;
                    [order[from], order[k]] = [order[k], order[from]];
                    void setMonitorOrder(order).then(() => monitorsList().then(setMons));
                  }}
                >
                  {k + 1}
                </button>
              ))}
            </div>
          )}
          </div>
        ))}
      </div>
      <h3 className="sub-h">Boyut</h3>
      <div className="slider">
        <label htmlFor="dscale" style={{ width: 130 }}>Dock boyutu</label>
        <input id="dscale" type="range" min={70} max={150} step={5} value={scale} onChange={(e) => { setScale(Number(e.target.value)); send({ scale: Number(e.target.value) / 100 }); }} />
        <span style={{ width: 44 }}>{scale}%</span>
      </div>
      <div className="slider">
        <label htmlFor="dgap" style={{ width: 130 }}>Kenardan uzaklık</label>
        <input id="dgap" type="range" min={0} max={24} step={1} value={gap} onChange={(e) => { setGap(Number(e.target.value)); send({ gap: Number(e.target.value) }); }} />
        <span style={{ width: 44 }}>{gap} px</span>
      </div>
      <h3 className="sub-h">Simgeler</h3>
      <IconSliders size={s.iconSize} gap={s.iconGap} />
      <p className="note">Boyut ve ekran seçimi hemen uygulanır. Seçili ekran bağlı değilse dock ana ekrana gelir.</p>
    </>
  );
}

/** Push-pin glyph: solid when the widget is pinned, outline otherwise. */
function PinIcon({ filled }: { filled: boolean }) {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill={filled ? "currentColor" : "none"} stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d="M15 3.5 20.5 9l-2.2.9-3.1 3.1.6 3.9-1.4 1.4-3.2-3.2L5 20l-1-1 4.9-5.4-3.2-3.2 1.4-1.4 3.9.6 3.1-3.1z" />
    </svg>
  );
}

function Desktop({ floating }: { floating: FloatCfg[] }) {
  return (
    <>
      <h2>Masaüstü</h2>
      <p className="sub">Masaüstüne aldığın widget'lar. Pencereyi tutamacından sürükleyip taşı, yerini beğenince sabitle.</p>
      {floating.map((f) => (
        <div key={f.id} className={f.pinned ? "card static" : "card static unpinned"}>
          <Preview id={f.id} zoom={0.85} />
          <div className="meta">
            <b>{WIDGET_INFO[f.id].label}</b>
            <span>
              Konum: x {f.x}, y {f.y} · {f.pinned ? "sabitlendi" : "taşınabilir"}
            </span>
          </div>
          <button className={f.pinned ? "keybtn rec" : "keybtn"} onClick={() => void floatPin(f.id, !f.pinned)} title={f.pinned ? "Sabitlemeyi kaldır" : "Bu konuma sabitle"}>
            <PinIcon filled={f.pinned} />
            {f.pinned ? "Sabitlendi" : "Sabitle"}
          </button>
          <button className="keybtn" onClick={() => void floatClose(f.id)}>
            Çubuğa ekle
          </button>
        </div>
      ))}
      {!floating.length && <p className="note">Henüz masaüstünde widget yok. Widget'lar sekmesinde "Masaüstüne al"a bas.</p>}
    </>
  );
}

const TEXT_SWATCHES = ["#ffffff", "#f4ede6", "#d9d9e0", "#15151a", "#ff8f3d", "#2ccfc6"];
const ACC_SWATCHES = ["#ff7a1a", "#0fb5ac", "#5b8cff", "#a78bfa", "#3fd07a", "#ff5a7a"];

/** Dock look: corner radius, border width, text color, accent. Sliders and the color picker follow the pointer at once; changes go out at most every 90 ms. */
function DockLook({ s }: { s: Cfg }) {
  const [r, setR] = useState(s.dockRadius);
  const [b, setB] = useState(s.dockBorder);
  const [text, setText] = useState(s.dockText);
  const [acc, setAcc] = useState(s.dockAccent);
  const t = useRef({ timer: 0, busy: 0, v: { radius: s.dockRadius, border: s.dockBorder, text: s.dockText, accent: s.dockAccent } });
  useEffect(() => {
    if (Date.now() - t.current.busy > 400) {
      setR(s.dockRadius);
      setB(s.dockBorder);
      setText(s.dockText);
      setAcc(s.dockAccent);
    }
  }, [s.dockRadius, s.dockBorder, s.dockText, s.dockAccent]);
  const send = (p: Partial<typeof t.current.v>) => {
    const x = t.current;
    x.v = { ...x.v, ...p };
    x.busy = Date.now();
    if (x.timer) return;
    x.timer = window.setTimeout(() => {
      x.timer = 0;
      void setLook(x.v.radius, x.v.border, x.v.text, x.v.accent);
    }, 90);
  };
  const colorRow = (label: string, value: string, swatches: string[], dflt: string, apply: (v: string) => void) => (
    <div className="colorrow">
      <span className="colorlab">{label}</span>
      <div className="accents">
        <button className={value === "" ? "accent sel" : "accent"} title="Temanın kendi rengi" style={{ background: dflt }} onClick={() => apply("")} />
        {swatches.map((c) => (
          <button key={c} className={value === c ? "accent sel" : "accent"} title={c} style={{ background: c }} onClick={() => apply(c)} />
        ))}
        <label className={value && !swatches.includes(value) ? "accent sel custom" : "accent custom"} title="Kendi rengini seç">
          <input type="color" value={value || dflt} onChange={(e) => apply(e.target.value)} />
        </label>
      </div>
    </div>
  );
  return (
    <>
      <h3 className="sub-h">Dock görünümü</h3>
      <div className="slider">
        <label htmlFor="drad" style={{ width: 150 }}>Köşe yuvarlaklığı</label>
        <input id="drad" type="range" min={0} max={32} step={1} value={r} onChange={(e) => { const v = Number(e.target.value); setR(v); send({ radius: v }); }} />
        <span style={{ width: 44 }}>{r} px</span>
      </div>
      <div className="slider">
        <label htmlFor="dbw" style={{ width: 150 }}>Kenar çizgisi kalınlığı</label>
        <input id="dbw" type="range" min={0} max={4} step={0.5} value={b} onChange={(e) => { const v = Number(e.target.value); setB(v); send({ border: v }); }} />
        <span style={{ width: 44 }}>{b} px</span>
      </div>
      {colorRow("Yazı rengi", text, TEXT_SWATCHES, "#ffffff", (v) => { setText(v); send({ text: v }); })}
      {colorRow("Dock vurgu rengi", acc, ACC_SWATCHES, "#7aa2ff", (v) => { setAcc(v); send({ accent: v }); })}
    </>
  );
}

function Design({ theme, startIcon, look, setLook: setAppLook, s }: { theme: ThemeCfg; startIcon: Cfg["startIcon"]; look: Look; setLook: (l: Look) => void; s: Cfg }) {
  // The slider follows the pointer at once; the (disk + every window) update goes out at most every 60 ms.
  const [op, setOp] = useState(Math.round(theme.opacity * 100));
  const drag = useRef({ busy: 0, sent: 0, timer: 0, last: 0 });
  useEffect(() => {
    if (Date.now() - drag.current.busy > 400) setOp(Math.round(theme.opacity * 100));
  }, [theme.opacity]);
  const slide = (v: number) => {
    const d = drag.current;
    setOp(v);
    d.busy = Date.now();
    d.last = v;
    if (d.timer) return;
    d.timer = window.setTimeout(() => {
      d.timer = 0;
      void setTheme({ ...theme, opacity: d.last / 100 });
    }, Math.max(0, 60 - (Date.now() - d.sent)));
    d.sent = Date.now();
  };
  return (
    <>
      <h2>Tasarım</h2>
      <p className="sub">Dock'un görünümünü seç.</p>
      <div className="presets">
        {PRESETS.map((p) => (
          <button key={p.id} className={theme.preset === p.id ? "preset sel" : "preset"} onClick={() => void setTheme({ ...theme, preset: p.id })}>
            <div className={`swatch sw-${p.id}`}>
              <i />
              <i />
              <i />
            </div>
            <b>{p.name}</b>
            <span>{p.desc}</span>
          </button>
        ))}
      </div>
      <div className="slider">
        <label htmlFor="op">Dolgu yoğunluğu</label>
        <input id="op" type="range" min={0} max={70} value={op} onChange={(e) => slide(Number(e.target.value))} />
        <span>{op}</span>
      </div>
      <h3 className="sub-h">Başlat simgesi</h3>
      <div className="icons-pick">
        {START_ICONS.map((i) => (
          <button key={i.id} className={(startIcon || "windows") === i.id ? "ipick sel" : "ipick"} onClick={() => void setStartIcon(i.id)}>
            <StartIcon kind={i.id} size={26} />
            <span>{i.name}</span>
          </button>
        ))}
      </div>
      <DockLook s={s} />
      <h3 className="sub-h">Uygulama teması</h3>
      <div className="keypick">
        {([["system", "Sisteme uy"], ["dark", "Koyu"], ["light", "Açık"]] as const).map(([k, n]) => (
          <button key={k} className={look.scheme === k ? "keybtn rec" : "keybtn"} onClick={() => setAppLook({ ...look, scheme: k })}>
            {n}
          </button>
        ))}
      </div>
      <h3 className="sub-h">Ayar penceresi vurgu rengi</h3>
      <div className="accents">
        <button className={look.accent == null ? "accent sel" : "accent"} title="Temaya göre (koyu: turuncu, açık: turkuaz)" style={{ background: "linear-gradient(135deg, #ff7a1a 50%, #0fb5ac 50%)" }} onClick={() => setAppLook({ ...look, accent: null })} />
        {ACCENTS.map((a, i) => (
          <button key={a.name} className={look.accent === i ? "accent sel" : "accent"} title={a.name} style={{ background: a.c[0] }} onClick={() => setAppLook({ ...look, accent: i })} />
        ))}
      </div>
    </>
  );
}

const KEY_BY_CODE: Record<string, string> = {
  ControlLeft: "lctrl", ControlRight: "rctrl", AltLeft: "lalt", AltRight: "ralt",
  ShiftLeft: "lshift", ShiftRight: "rshift", MetaLeft: "lwin", MetaRight: "rwin",
};
const KEY_NAME: Record<string, string> = {
  always: "Her tıklamada", lctrl: "Sol Ctrl", rctrl: "Sağ Ctrl", lalt: "Sol Alt", ralt: "Sağ Alt",
  lshift: "Sol Shift", rshift: "Sağ Shift", lwin: "Sol Win", rwin: "Sağ Win",
};

// Press-a-key recorder: click, then press the modifier (left/right are told apart by KeyboardEvent.code).
function KeyPicker({ value }: { value: string }) {
  const [rec, setRec] = useState(false);
  useEffect(() => {
    if (!rec) return;
    const down = (e: KeyboardEvent) => {
      e.preventDefault();
      if (e.code === "Escape") return setRec(false);
      const k = KEY_BY_CODE[e.code];
      if (k) {
        void setBringKey(k);
        setRec(false);
      }
    };
    window.addEventListener("keydown", down);
    return () => window.removeEventListener("keydown", down);
  }, [rec]);
  return (
    <div className="keypick">
      <button className={rec ? "keybtn rec" : "keybtn"} onClick={() => setRec(!rec)} onBlur={() => setRec(false)}>
        {rec ? "Bir tuşa bas… (Esc: vazgeç)" : KEY_NAME[value] ?? "Her tıklamada"}
      </button>
      {value !== "always" && value !== "" && (
        <button className="keybtn" onClick={() => void setBringKey("always")}>
          Sıfırla
        </button>
      )}
    </div>
  );
}

const mb = (b: number) => `${(b / 1048576).toFixed(1)} MB`;

/** Version, check for updates, install (download, replace, restart) and the automatic options. */
function Updates({ s }: { s: Cfg }) {
  const [u, setU] = useState<UpdateInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [prog, setProg] = useState<{ done: number; total: number } | null>(null);
  const [err, setErr] = useState("");
  useEffect(() => {
    void updateState().then(setU);
    const a = listen<UpdateInfo>("update://state", (e) => setU(e.payload));
    const b = listen<{ done: number; total: number }>("update://progress", (e) => setProg(e.payload));
    return () => {
      void a.then((f) => f());
      void b.then((f) => f());
    };
  }, []);
  const check = () => {
    setBusy(true);
    setErr("");
    updateCheck().then(setU).finally(() => setBusy(false));
  };
  const install = () => {
    setBusy(true);
    setErr("");
    setProg({ done: 0, total: u?.size ?? 0 });
    updateInstall().catch((e) => {
      setErr(String(e));
      setBusy(false);
      setProg(null);
    });
  };
  const status = !u ? "" : busy && prog ? "İndiriliyor…" : u.error ? u.error : u.available ? `Yeni sürüm hazır: ${u.latest} (şu an ${u.current})` : u.checked ? `Güncelsin (${u.current})` : `Sürüm ${u.current}`;
  return (
    <div className="card static" style={{ flexWrap: "wrap" }}>
      <div className="meta">
        <b>Güncellemeler</b>
        <span className={u?.error || err ? "bk err" : u?.available ? "bk ok" : ""}>{err || status}</span>
        {u?.available && u.notes && <span className="upnotes">{u.notes.split("\n").filter((l) => l.trim().startsWith("-") || l.trim().startsWith("*")).slice(0, 4).join("\n")}</span>}
        {busy && prog && prog.total > 0 && (
          <span className="upbar">
            <i style={{ width: `${Math.min(100, (prog.done / prog.total) * 100)}%` }} />
          </span>
        )}
        {busy && prog && <span>{mb(prog.done)} / {mb(prog.total)} · bitince uygulama kendini yeniden başlatır</span>}
      </div>
      <div className="keypick">
        <button className="keybtn" disabled={busy} onClick={check}>
          {busy && !prog ? "Denetleniyor…" : "Güncellemeleri denetle"}
        </button>
        {u?.available && u.canInstall && (
          <button className="keybtn rec" disabled={busy} onClick={install}>
            İndir ve yükle{u.size ? ` (${mb(u.size)})` : ""}
          </button>
        )}
      </div>
      <div className="upopts">
        <label>
          <Switch on={s.autoUpdateCheck} label="Güncellemeleri kendiliğinden denetle" onChange={(v) => void setAutoUpdate(v, s.autoUpdateInstall)} />
          Kendiliğinden denetle
        </label>
        <label className={s.autoUpdateCheck ? "" : "off"}>
          <Switch on={s.autoUpdateInstall} label="Yeni sürümü kendiliğinden yükle" onChange={(v) => void setAutoUpdate(s.autoUpdateCheck, v)} />
          Yeni sürümü sormadan yükle ve yeniden başlat
        </label>
      </div>
    </div>
  );
}

/** Export / import of every setting (and the sticky notes) as one file. */
function Backup() {
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const run = (f: () => Promise<string>, done: (r: string) => string) => {
    setBusy(true);
    setMsg(null);
    f()
      .then((r) => setMsg(r ? { ok: true, text: done(r) } : null))
      .catch((e) => setMsg({ ok: false, text: String(e) }))
      .finally(() => setBusy(false));
  };
  return (
    <div className="card static">
      <div className="meta">
        <b>Ayar yedeği</b>
        <span>Dock düzeni, tasarım, masaüstü widget'ları, boyutlar ve yapışkan notlar tek bir dosyaya kaydedilir. Yedekten yüklersen şimdiki ayarların üzerine yazılır; öncekiler config.before-import.json olarak saklanır.</span>
        {msg && <span className={msg.ok ? "bk ok" : "bk err"}>{msg.text}</span>}
      </div>
      <div className="keypick">
        <button className="keybtn" disabled={busy} onClick={() => run(backupExport, (p) => `Kaydedildi: ${p}`)}>
          Dışa aktar
        </button>
        <button className="keybtn" disabled={busy} onClick={() => run(backupImport, (m) => m)}>
          Yedekten yükle
        </button>
      </div>
    </div>
  );
}

function Behavior({ bringToMain, bringKey, indicator, hoverLift, s }: { bringToMain: boolean; bringKey: string; indicator: Cfg["indicator"]; hoverLift: boolean; s: Cfg }) {
  return (
    <>
      <h2>Davranış</h2>
      <p className="sub">Dock'taki uygulamalara tıklayınca ne olacağını seç.</p>
      <div className="card static">
        <div className="meta">
          <b>Uygulamayı ana ekrana getir</b>
          <span>Dock'tan bir uygulamaya tıklayınca penceresi başka ekrandaysa ana ekrana taşınır.</span>
        </div>
        <Switch on={bringToMain} onChange={(v) => void setBringToMain(v)} label="Uygulamayı ana ekrana getir" />
      </div>
      <div className="card static">
        <div className="meta">
          <b>Simgeler üzerine gelince büyüsün</b>
          <span>Fareyi dock'taki uygulamaların üzerine getirince simge yukarı kalkıp büyür, yanındakiler hafifçe uyum sağlar.</span>
        </div>
        <Switch on={hoverLift} onChange={(v) => void setHoverLift(v)} label="Simgeler üzerine gelince büyüsün" />
      </div>
      <div className="card static">
        <div className="meta">
          <b>Açık uygulama göstergesi</b>
          <span>Açık uygulamaların simgesinin altındaki işaret. Her pencere için bir işaret çıkar.</span>
        </div>
        <div className="keypick">
          {(["dot", "bigdot", "line"] as const).map((k) => (
            <button key={k} className={(indicator || "dot") === k ? "keybtn rec" : "keybtn"} onClick={() => void setIndicator(k)}>
              {k === "dot" ? "Nokta" : k === "bigdot" ? "Büyük nokta" : "Çizgi"}
            </button>
          ))}
        </div>
      </div>
      <div className="card static">
        <div className="meta">
          <b>Kısayol: Win + 1…0</b>
          <span>Dock'taki sıraya göre uygulamayı açar: Win+1 ilk uygulama, Win+0 onuncu. Açıksa öne getirir.</span>
        </div>
      </div>
      <Updates s={s} />
      <Backup />
      <div className="card static">
        <div className="meta">
          <b>Görev çubuğu</b>
          <span>Yeniden başlat: sadece görev çubuğu (dock) yenilenir, Aterya açık kalır. Kapat: Aterya tamamen kapanır, Windows görev çubuğu geri gelir.</span>
        </div>
        <div className="keypick">
          <button className="keybtn" onClick={() => void restartApp()}>
            Görev çubuğunu yeniden başlat
          </button>
          <button className="keybtn danger" onClick={() => void quitApp()}>
            Aterya'yı kapat
          </button>
        </div>
      </div>
      {bringToMain && (
        <div className="card static">
          <div className="meta">
            <b>Ne zaman</b>
            <span>Kutuya tıkla, sonra bir tuşa bas (Ctrl, Alt, Shift ya da Win; sol ve sağ ayrı). O tuşa basılıyken tıklayınca taşınır.</span>
          </div>
          <KeyPicker value={bringKey || "always"} />
        </div>
      )}
    </>
  );
}

const ico = { width: 12, height: 12, viewBox: "0 0 12 12", fill: "none", stroke: "currentColor", strokeWidth: 1.2, strokeLinecap: "round" as const, strokeLinejoin: "round" as const };

function TitleBar() {
  const w = getCurrentWindow();
  const [max, setMax] = useState(false);
  useEffect(() => {
    const sync = () => void w.isMaximized().then(setMax);
    sync();
    const un = w.onResized(sync);
    return () => void un.then((f) => f());
  }, [w]);
  return (
    <div className="tb" data-tauri-drag-region>
      <img className="tb-logo" src="/aterya.svg" alt="" width={18} height={18} draggable={false} />
      <span className="tb-title" data-tauri-drag-region>
        ATERYA
      </span>
      <div className="tb-btns">
        <button aria-label="Küçült" onClick={() => void w.minimize()}>
          <svg {...ico}><path d="M2 6h8" /></svg>
        </button>
        <button aria-label={max ? "Önceki boyut" : "Büyüt"} onClick={() => void w.toggleMaximize()}>
          {max ? (
            <svg {...ico}><rect x="2" y="3.5" width="6.5" height="6.5" rx="1.5" /><path d="M4 3.5V3a1 1 0 011-1h3.5A1.5 1.5 0 0110 3.5V7a1 1 0 01-1 1h-.5" /></svg>
          ) : (
            <svg {...ico}><rect x="2" y="2" width="8" height="8" rx="2" /></svg>
          )}
        </button>
        <button className="close" aria-label="Kapat" onClick={() => void w.close()}>
          <svg {...ico}><path d="M2.5 2.5l7 7M9.5 2.5l-7 7" /></svg>
        </button>
      </div>
    </div>
  );
}

export function Settings() {
  demo.on = true; // widget previews show sample values, not live readings
  const s = useSettings();
  const [look, setLook] = useLook();
  const [tab, setTab] = useState<Tab>("widgets");
  useEffect(() => {
    document.title = "Aterya";
  }, []);
  useEffect(() => {
    const t = setTimeout(() => void requestState(), 400); // live previews missed the backend events
    return () => clearTimeout(t);
  }, [tab]);
  if (!s) return null;
  return (
    <div className="st-shell">
      <TitleBar />
      <div className="st">
      <nav>
        <div className="brand">
          <img src="/aterya.svg" alt="" width={30} height={30} draggable={false} />
          Aterya
        </div>
        {(Object.keys(TAB_NAME) as Tab[]).map((t) => (
          <button key={t} className={tab === t ? "tab on" : "tab"} onClick={() => setTab(t)}>
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
              <path d={NAV_ICON[t]} />
            </svg>
            {TAB_NAME[t]}
          </button>
        ))}
        <div className="grow" />
        <div className="navfoot">
          <div className="live">
            <i />
            Dock çalışıyor · {s.layout.order.filter((id) => !s.layout.hidden.includes(id)).length} widget
          </div>
          <button className="keybtn" onClick={() => void restartApp()}>
            Dock'u yeniden başlat
          </button>
        </div>
      </nav>
      <main>{tab === "widgets" ? <Catalog layout={s.layout} floating={s.floating} goDesktop={() => setTab("desktop")} scales={s.scales} brightnessLayout={s.brightnessLayout} /> : tab === "order" ? <Order layout={s.layout} floating={s.floating} layout_scales={s.scales} dividerWidth={s.dividerWidth} /> : tab === "dock" ? <DockTab s={s} /> : tab === "desktop" ? <Desktop floating={s.floating} /> : tab === "design" ? <Design theme={s.theme} startIcon={s.startIcon} look={look} setLook={setLook} s={s} /> : <Behavior bringToMain={s.bringToMain} bringKey={s.bringKey} indicator={s.indicator} hoverLift={s.hoverLift} s={s} />}</main>
      </div>
    </div>
  );
}
