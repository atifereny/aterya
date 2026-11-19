import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { tipReady } from "../lib/ipc";

type TipData = { text: string; sub: string; icon: string };

/** The name tooltip window: icon, name and a hint line. It reports its size so the backend can place it above the icon. */
export function Tip() {
  const [t, setT] = useState<TipData | null>(null);
  const el = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const un = listen<TipData>("tip://show", (e) => setT(e.payload));
    return () => void un.then((f) => f());
  }, []);
  useLayoutEffect(() => {
    if (!t || !el.current) return;
    const r = el.current.getBoundingClientRect();
    void tipReady(r.width, r.height);
  }, [t]);
  if (!t) return null;
  return (
    <div className="tipw" ref={el} key={t.text + t.sub}>
      <div className="tip">
        {t.icon && <img src={t.icon} width={22} height={22} alt="" draggable={false} />}
        <div className="tip-col">
          <b>{t.text}</b>
          {t.sub && <span>{t.sub}</span>}
        </div>
      </div>
      <i className="tip-arrow" />
    </div>
  );
}
