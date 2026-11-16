import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { EVENTS, requestState, setPillBounds } from "./lib/ipc";
import { AppRow } from "./apps/AppRow";
import { NowPlaying } from "./widgets/NowPlaying";
import { Weather } from "./widgets/Weather";
import { ClaudeUsage } from "./widgets/ClaudeUsage";
import { Batteries } from "./widgets/Batteries";
import { Tray } from "./widgets/Tray";
import { SystemControls } from "./widgets/SystemControls";
import { Clock } from "./widgets/Clock";

const CAP = 0.9; // pill never wider than 90% of the window
const CHROME_PX = 12 * 2 + 2 + 8 + 8; // pill padding + border + gaps between the app row and the other group

export default function App() {
  const pill = useRef<HTMLDivElement>(null);
  const others = useRef<HTMLDivElement>(null);
  const [availablePx, setAvailable] = useState(600);
  useEffect(() => {
    const el = pill.current!;
    const report = () => {
      const r = el.getBoundingClientRect();
      void setPillBounds(r.x, r.width, r.height).then(solid);
      setAvailable(Math.max(0, window.innerWidth * CAP - CHROME_PX - others.current!.getBoundingClientRect().width));
    };
    const solid = (blur: boolean) => el.classList.toggle("solid", !blur);
    const unlisten = listen<boolean>(EVENTS.glass, (e) => solid(e.payload)); // transparency toggled live
    const ro = new ResizeObserver(report);
    ro.observe(el);
    ro.observe(others.current!); // widgets grow/shrink independently of the pill
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
  return (
    <div id="dock">
      <div className="pill" ref={pill}>
        <AppRow availablePx={availablePx} />
        <div className="dock-group" ref={others}>
          <div className="sep" />
          <NowPlaying />
          <Weather />
          <ClaudeUsage />
          <Batteries />
          <div className="sep" />
          <Tray />
          <SystemControls />
          <Clock />
        </div>
      </div>
    </div>
  );
}
