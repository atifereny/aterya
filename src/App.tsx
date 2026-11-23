import { Fragment, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { EVENTS, openSettings, requestState, setPillBounds, useSettings } from "./lib/ipc";
import { WIDGET_INFO } from "./lib/widgets";
import { AppRow } from "./apps/AppRow";
import { Zoom } from "./lib/Zoom";
import { applyLook } from "./lib/look";
import { isVertical } from "./lib/axis";

const CAP = 0.9; // pill never wider than 90% of the window
const CHROME_PX = 12 * 2 + 2; // pill padding + border
const GAP_PX = 8;
/** Widgets a side dock shows without a compact version of their own: they already fit a narrow column (CSS stacks them). */
const SIDE_PLAIN: string[] = ["apps", "tray", "system", "notifications", "showdesktop", "keyboardlang", "microphone", "recyclebin", "stickynotes", "clipboard", "batteries", "batteryring", "cpuring", "gpuring"];
const sideOk = (id: string) => SIDE_PLAIN.includes(id) || !!WIDGET_INFO[id as keyof typeof WIDGET_INFO]?.side;

/** Length (along the dock) the pill's other children take (margins and gaps included), apps excluded. */
function othersExtent(pill: HTMLElement): number {
  const vert = isVertical();
  const kids = [...pill.children] as HTMLElement[];
  let w = 0;
  for (const k of kids) {
    if (k.hasAttribute("data-apps")) continue;
    const cs = getComputedStyle(k);
    const r = k.getBoundingClientRect();
    w += vert ? r.height + parseFloat(cs.marginTop) + parseFloat(cs.marginBottom) : r.width + parseFloat(cs.marginLeft) + parseFloat(cs.marginRight);
  }
  return w + GAP_PX * Math.max(0, kids.length - 1);
}

export default function App() {
  const pill = useRef<HTMLDivElement>(null);
  const [availablePx, setAvailable] = useState(600);
  const settings = useSettings();
  const side = settings?.dockEdge === "left" || settings?.dockEdge === "right";
  const shown = (settings?.layout.order ?? []).filter((id) => !settings!.layout.hidden.includes(id) && !settings!.floating.some((f) => f.id === id) && (!side || sideOk(id)));
  useEffect(() => {
    const el = pill.current!;
    const report = () => {
      const r = el.getBoundingClientRect();
      void setPillBounds(r.x, r.y, r.width, r.height).then(solid);
      setAvailable(Math.max(0, (isVertical() ? window.innerHeight : window.innerWidth) * CAP - CHROME_PX - othersExtent(el)));
    };
    const solid = (blur: boolean) => el.classList.toggle("solid", !blur);
    const unlisten = listen<boolean>(EVENTS.glass, (e) => solid(e.payload)); // transparency toggled live
    const ro = new ResizeObserver(report);
    ro.observe(el);
    window.addEventListener("resize", report); // width changes move x without resizing the pill
    // shortcut: listen() registers asynchronously; replay after a short delay so child listeners are live. Upgrade to awaiting all listeners.
    const replay = setTimeout(() => void requestState(), 300);
    return () => {
      clearTimeout(replay);
      ro.disconnect();
      void unlisten.then((f) => f());
      window.removeEventListener("resize", report);
    };
  }, []);
  const shownKey = shown.join();
  useEffect(() => {
    // Widgets grow/shrink independently of the pill: re-measure when any of them resizes.
    const el = pill.current!;
    const ro = new ResizeObserver(() => window.dispatchEvent(new Event("resize")));
    for (const k of el.children) if (!k.hasAttribute("data-apps")) ro.observe(k);
    return () => ro.disconnect();
  }, [shownKey]);
  useEffect(() => {
    // Widgets that were just mounted missed their backend events; replay the cache.
    const t = setTimeout(() => void requestState(), 150);
    return () => clearTimeout(t);
  }, [shownKey]);
  useEffect(() => {
    if (!settings) return;
    document.documentElement.dataset.theme = settings.theme.preset;
    document.documentElement.dataset.edge = settings.dockEdge;
    document.documentElement.dataset.lift = settings.hoverLift ? "on" : "off";
    applyLook(settings);
    document.documentElement.style.setProperty("--div-w", `${settings.dividerWidth}px`);
    document.documentElement.style.setProperty("--fill", String(settings.theme.opacity));
  }, [settings]);
  return (
    <div id="dock">
      <div className="pill" ref={pill} onContextMenu={(e) => { e.preventDefault(); void openSettings(); }}>
        {shown.map((id, i) => (
          <Fragment key={id}>
            {i > 0 && settings?.layout.dividers?.includes(shown[i - 1]) && <div className="sep" />}
            {id === "apps" ? <AppRow availablePx={availablePx} /> : <Zoom scale={settings?.scales[id]}>{side && WIDGET_INFO[id].side ? WIDGET_INFO[id].side!() : WIDGET_INFO[id].render()}</Zoom>}
          </Fragment>
        ))}
      </div>
    </div>
  );
}
