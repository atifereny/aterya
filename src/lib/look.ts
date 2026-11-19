import type { Settings } from "./ipc";

/** Applies the dock look settings (radius, border width, text color, accent) as CSS variables on <html>. */
export function applyLook(s: Settings) {
  const r = document.documentElement;
  r.style.setProperty("--dock-r", `${s.dockRadius}px`);
  r.style.setProperty("--dock-bw", `${s.dockBorder}px`);
  for (const [key, attr, v] of [["--dock-fg", "fg", s.dockText], ["--dock-acc", "acc", s.dockAccent]] as const) {
    if (v) {
      r.style.setProperty(key, v);
      r.dataset[attr] = "on";
    } else {
      r.style.removeProperty(key);
      delete r.dataset[attr];
    }
  }
}
