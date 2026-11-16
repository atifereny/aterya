import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface AppEntry {
  id: string;
  name: string;
  iconDataUrl: string;
  pinned: boolean;
  running: boolean;
  focused: boolean;
  windowCount: number;
}
export interface TrayIcon {
  id: string;
  tooltip: string;
  iconDataUrl: string;
}
export interface MediaState {
  title: string;
  artist: string;
  artDataUrl: string | null;
  playing: boolean;
}
export interface Weather {
  tempC: number;
  code: number;
  city: string;
}
export type ClaudeUsage =
  | { kind: "ok"; fiveHour: number; weekly: number }
  | { kind: "expired" }
  | { kind: "unknown" };
export interface DeviceBattery {
  id: "mouse" | "headset" | "keyboard";
  name: string;
  percent: number | null;
  charging: boolean;
  connected: boolean;
}
export interface SystemState {
  volume: number;
  muted: boolean;
  network: "ethernet" | "wifi" | "offline";
}

export const EVENTS = {
  apps: "apps://changed",
  tray: "tray://changed",
  media: "media://changed",
  weather: "weather://changed",
  claude: "claude://changed",
  battery: "battery://changed",
  system: "system://changed",
  visibility: "dock://visibility",
  glass: "dock://glass", // payload: boolean, whether DWM blur is live
} as const;

export interface DockVisibility {
  hidden: boolean;
}

/** Latest payload of a backend event; `initial` until the first one arrives. */
export function useEvent<T>(name: string, initial: T): T {
  const [value, setValue] = useState<T>(initial);
  useEffect(() => {
    const un = listen<T>(name, (e) => setValue(e.payload));
    return () => {
      un.then((f) => f());
    };
  }, [name]);
  return value;
}

export const requestState = () => invoke<void>("request_state");
export const appClick = (id: string) => invoke<void>("app_click", { id });
export const startMenu = () => invoke<void>("start_menu");
export const trayClick = (id: string, button: string, x: number, y: number) =>
  invoke<void>("tray_click", { id, button, x, y });
export const openTrayOverflow = () => invoke<void>("open_tray_overflow");
export const mediaControl = (action: "prev" | "toggle" | "next") =>
  invoke<void>("media_control", { action });
export const setVolume = (value: number) => invoke<void>("set_volume", { value });
export const toggleMute = () => invoke<void>("toggle_mute");
export const openQuickSettings = () => invoke<void>("open_quick_settings");
export const openNotifications = () => invoke<void>("open_notifications");
/** Resolves to whether DWM blur is live behind the pill. */
export const setPillBounds = (x: number, width: number, height: number) =>
  invoke<boolean>("set_pill_bounds", { x, width, height });

export interface PreviewItem {
  hwnd: string;
  title: string;
  minimized: boolean;
  x: number;
  y: number;
}
export interface PreviewPayload {
  items: PreviewItem[];
  icon: string;
  thumbW: number;
  thumbH: number;
  titleH: number;
}
export const previewHover = (groupId: string, iconCx: number) => invoke<void>("preview_hover", { groupId, iconCx });
export const previewLeave = () => invoke<void>("preview_leave");
export const previewEnter = () => invoke<void>("preview_enter");
export const previewItems = () => invoke<PreviewPayload | null>("preview_items");
export const activateWindow = (hwnd: string) => invoke<void>("activate_window", { hwnd });
export const closeWindow = (hwnd: string) => invoke<void>("close_window", { hwnd });
