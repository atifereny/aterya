import { useState } from "react";
import { hwinfoRestart } from "../lib/ipc";

/** Shows on hover over a widget that reads HWiNFO: closes HWiNFO and starts it again (after a crash, or when it stops sharing data). */
export function HwRefresh() {
  const [state, setState] = useState<"idle" | "busy" | "ok" | "err">("idle");
  const [msg, setMsg] = useState("");
  const press = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (state === "busy") return;
    setState("busy");
    hwinfoRestart()
      .then(() => setState("ok"))
      .catch((x) => {
        setMsg(String(x));
        setState("err");
      })
      .finally(() => setTimeout(() => setState("idle"), 4000));
  };
  const title = state === "err" ? msg : state === "ok" ? "HWiNFO yeniden başlatılıyor, birkaç saniye içinde değerler gelir" : "HWiNFO'yu yeniden başlat";
  return (
    <button className={`hw-refresh ${state}`} onClick={press} title={title} aria-label="HWiNFO'yu yeniden başlat">
      {state === "err" ? (
        <b>!</b>
      ) : (
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
          <path d="M20 11a8 8 0 10-2.3 5.7M20 4v7h-7" />
        </svg>
      )}
    </button>
  );
}
