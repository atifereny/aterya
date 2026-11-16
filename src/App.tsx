import { useEffect, useRef } from "react";
import { setPillBounds } from "./lib/ipc";

// Static copy of mockup A; Task 14 swaps in the live widgets.
export default function App() {
  const pill = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = pill.current!;
    const report = () => {
      const r = el.getBoundingClientRect();
      void setPillBounds(r.x, r.width, r.height).then((blur) => el.classList.toggle("solid", !blur));
    };
    const ro = new ResizeObserver(report);
    ro.observe(el);
    window.addEventListener("resize", report); // width changes move x without resizing the pill
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", report);
    };
  }, []);
  return (
    <div id="dock">
      <div className="pill" ref={pill}>
        <div className="app" style={{ background: "linear-gradient(135deg,#3b82f6,#06b6d4)" }}>⊞</div>
        <div className="app on" style={{ background: "#e8710a" }}>C</div>
        <div className="app on" style={{ background: "#2563eb" }}>V</div>
        <div className="app" style={{ background: "#5865f2" }}>D</div>
        <div className="app on" style={{ background: "#1db954" }}>S</div>
        <div className="sep" />
        <div className="tile"><div className="art" /><div><div>Blinding Lights</div><div className="dim">The Weeknd</div></div></div>
        <div className="tile"><div className="big">14°</div><div className="dim">İstanbul<br />Parçalı</div></div>
        <div className="tile"><div><div className="dim">Claude 5s</div><div className="bar"><i style={{ width: "62%", background: "#f59e0b" }} /></div></div></div>
        <div className="tile"><div className="big">21:47</div><div className="dim">Pzt<br />16 Kas</div></div>
      </div>
    </div>
  );
}
