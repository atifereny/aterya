import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { StartIconKind } from "./StartIcon";

export interface AppEntry {
  id: string;
  name: string;
  iconDataUrl: string;
  pinned: boolean;
  running: boolean;
  focused: boolean;
  windowCount: number;
  badge: number;
  attention: boolean;
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
  | { kind: "ok"; fiveHour: number; weekly: number; fiveHourResets: string | null; weeklyResets: string | null; topModel: { name: string; tokens: number } | null }
  | { kind: "expired" }
  | { kind: "limited" }
  | { kind: "refreshing" }
  | { kind: "unknown" };
export interface DeviceBattery {
  id: "mouse" | "headset" | "keyboard";
  name: string;
  percent: number | null;
  charging: boolean;
  connected: boolean;
  asleep: boolean; // last known value of a sleeping keyboard
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
  trayFlyout: "tray://flyout", // payload: boolean, overflow flyout open
  mixerOpen: "mixer://open", // payload: boolean
  settings: "settings://changed", // payload: Settings
  langOpen: "lang://open",
  glass: "dock://glass", // payload: boolean, whether DWM blur is live
} as const;

export interface DockVisibility {
  hidden: boolean;
}

/** On in the settings window: its widget previews show sample values instead of live readings. */
export const demo = { on: false };

const DEMO_EVENTS: Record<string, unknown> = {
  [EVENTS.battery]: [
    { id: "mouse", name: "Fare", percent: 86, charging: false, connected: true, asleep: false },
    { id: "headset", name: "Kulaklık", percent: 43, charging: false, connected: true, asleep: false },
    { id: "keyboard", name: "Klavye", percent: 35, charging: false, connected: true, asleep: false },
  ],
  [EVENTS.claude]: { kind: "ok", fiveHour: 42, weekly: 18, fiveHourResets: null, weeklyResets: null, topModel: null },
  [EVENTS.media]: { title: "Parça adı", artist: "Sanatçı", artDataUrl: null, playing: true },
  [EVENTS.weather]: { tempC: 17, code: 2, city: "İstanbul" },
  [EVENTS.system]: { volume: 50, muted: false, network: "wifi" },
  [EVENTS.tray]: [],
  [EVENTS.trayFlyout]: false,
  [EVENTS.mixerOpen]: false,
  [EVENTS.langOpen]: false,
};

/** Latest payload of a backend event; `initial` until the first one arrives. */
export function useEvent<T>(name: string, initial: T): T {
  const [value, setValue] = useState<T>(initial);
  const fake = demo.on && name in DEMO_EVENTS;
  useEffect(() => {
    if (fake) return;
    const un = listen<T>(name, (e) => setValue(e.payload));
    return () => {
      un.then((f) => f());
    };
  }, [name, fake]);
  return fake ? (DEMO_EVENTS[name] as T) : value;
}

/** The real call, or `fake` while previewing. */
const sample = <T,>(real: () => Promise<T>, fake: T): Promise<T> => (demo.on ? Promise.resolve(fake) : real());

export const requestState = () => invoke<void>("request_state");
export const appClick = (id: string) => invoke<void>("app_click", { id });
export const startMenu = () => invoke<void>("start_menu");
export const trayClick = (id: string, button: string, x: number, y: number) =>
  invoke<void>("tray_click", { id, button, x, y });
export const openTrayOverflow = (x: number, y: number, wasOpen: boolean) =>
  invoke<void>("open_tray_overflow", { x, y, wasOpen });
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

export interface MixerApp {
  id: string;
  name: string;
  icon: string;
  volume: number;
  muted: boolean;
}
export const mixerList = () => invoke<MixerApp[]>("mixer_list");
export const mixerSetVolume = (id: string, value: number) => invoke<void>("mixer_set_volume", { id, value });
export const mixerSetMute = (id: string, muted: boolean) => invoke<void>("mixer_set_mute", { id, muted });
export const mixerToggle = (iconCx: number, rows: number, wasOpen: boolean) =>
  invoke<void>("mixer_toggle", { iconCx, rows, wasOpen });
export const mixerFit = (rows: number) => invoke<void>("mixer_fit", { rows });

export type WidgetId = "apps" | "nowplaying" | "weather" | "claude" | "batteries" | "tray" | "system" | "clock" | "screentime" | "topcpu" | "topram" | "brightness" | "notifications" | "showdesktop" | "batteryring" | "cpuring" | "gpuring" | "memory" | "storage" | "netspeed" | "keyboardlang" | "microphone" | "recyclebin" | "stickynotes" | "clipboard";
export interface LayoutCfg {
  order: WidgetId[];
  hidden: WidgetId[];
  /** Widgets followed by a divider line on the dock. */
  dividers: WidgetId[];
}
export interface ThemeCfg {
  preset: "glass" | "dark" | "light";
  opacity: number;
}
export interface FloatCfg {
  id: WidgetId;
  x: number;
  y: number;
  pinned: boolean;
}
export interface Settings {
  layout: LayoutCfg;
  theme: ThemeCfg;
  bringToMain: boolean;
  bringKey: string;
  floating: FloatCfg[];
  scales: Partial<Record<WidgetId, number>>;
  brightnessLayout: "row" | "column";
  indicator: "dot" | "bigdot" | "line";
  startIcon: StartIconKind;
  dockMonitor: string;
  dockScale: number;
  dockGap: number;
  dockAutohide: boolean;
  hoverLift: boolean;
  dividerWidth: number;
  iconSize: number;
  iconGap: number;
  dockRadius: number;
  dockBorder: number;
  dockText: string;
  dockAccent: string;
  autoUpdateCheck: boolean;
  autoUpdateInstall: boolean;
}
export const setLook = (radius: number, border: number, text: string, accent: string) => invoke<void>("set_look", { radius, border, text, accent });
export const setIcons = (size: number, gap: number) => invoke<void>("set_icons", { size, gap });
export const setDividerWidth = (value: number) => invoke<void>("set_divider_width", { value });
export const setHoverLift = (on: boolean) => invoke<void>("set_hover_lift", { on });
export const setAutohide = (on: boolean) => invoke<void>("set_autohide", { on });
export type MonitorInfo = { device: string; x: number; y: number; width: number; height: number; primary: boolean; num: number };
export const setMonitorOrder = (order: string[]) => invoke<void>("set_monitor_order", { order });
export const monitorsList = () => invoke<MonitorInfo[]>("monitors_list");
export const setDock = (monitor: string, scale: number, gap: number) => invoke<void>("set_dock", { monitor, scale, gap });
export const autostartGet = () => invoke<boolean>("autostart_get");
export const autostartSet = (on: boolean) => invoke<void>("autostart_set", { on });
export const setStartIcon = (value: StartIconKind) => invoke<void>("set_start_icon", { value });
export const setIndicator = (value: "dot" | "bigdot" | "line") => invoke<void>("set_indicator", { value });
export const setWidgetScale = (id: WidgetId, scale: number) => invoke<void>("set_widget_scale", { id, scale });
export const setBrightnessLayout = (value: "row" | "column") => invoke<void>("set_brightness_layout", { value });
export const floatOpen = (id: WidgetId) => invoke<void>("float_open", { id });
export const floatClose = (id: WidgetId) => invoke<void>("float_close", { id });
export const floatPin = (id: WidgetId, pinned: boolean) => invoke<void>("float_pin", { id, pinned });
export const getSettings = () => invoke<Settings>("get_settings");
export const setLayout = (layout: LayoutCfg) => invoke<void>("set_layout", { layout });
export const setTheme = (theme: ThemeCfg) => invoke<void>("set_theme", { theme });
export const setBringToMain = (value: boolean) => invoke<void>("set_bring_to_main", { value });
export const setBringKey = (value: string) => invoke<void>("set_bring_key", { value });
export const showDesktop = () => invoke<void>("show_desktop");
export const openSettings = () => invoke<void>("open_settings");

/** Current settings: loaded once, then kept live from `settings://changed`. */
export function useSettings(): Settings | null {
  const [s, setS] = useState<Settings | null>(null);
  useEffect(() => {
    void getSettings().then((v) => setS((cur) => cur ?? v));
    const un = listen<Settings>(EVENTS.settings, (e) => setS(e.payload));
    return () => void un.then((f) => f());
  }, []);
  return s;
}
export const claudeRefresh = () => invoke<void>("claude_refresh");

export const restartApp = () => invoke<void>("restart_app");
export const quitApp = () => invoke<void>("quit_app");
export type Display = { index: number; percent: number };
export const brightnessGet = () => sample(() => invoke<Display[]>("brightness_get"), [{ index: 0, percent: 100 }, { index: 1, percent: 100 }, { index: 2, percent: 100 }]);
export const brightnessSet = (index: number, percent: number) => invoke<void>("brightness_set", { index, percent });
export type Chip = { load?: number | null; temp?: number | null; power?: number | null };
export type Mem = { used: number; total: number };
export type Disk = { name: string; temp?: number | null; read: number; write: number };
export type Sensors = { cpu: Chip; gpu: Chip; ram: Mem; vram?: Mem | null; down: number; up: number; disks: Disk[]; hwinfo: boolean };
export type Lang = { code: string; name: string };
export type Layout = { hkl: string; code: string; name: string; current: boolean };
export const langList = () => invoke<Layout[]>("lang_list");
export const outdevToggle = (iconCx: number, wasOpen: boolean) => invoke<void>("outdev_pop_toggle", { iconCx, wasOpen });
export const appMenuToggle = (iconCx: number, id: string, wasOpen: boolean) => invoke<void>("app_menu_toggle", { iconCx, id, wasOpen });
export const tipShow = (text: string, sub: string, icon: string, iconCx: number) => invoke<void>("tip_show", { text, sub, icon, iconCx });
export const tipReady = (width: number, height: number) => invoke<void>("tip_ready", { width, height });
export const tipHide = () => invoke<void>("tip_hide");
export type MicState = { available: boolean; muted: boolean };
export const micGet = () => sample(() => invoke<MicState>("mic_get"), { available: true, muted: false });
export const micToggle = () => invoke<MicState>("mic_toggle");
export type BinState = { items: number; bytes: number };
export const binGet = () => sample(() => invoke<BinState>("bin_get"), { items: 12, bytes: 3.4e8 });
export const binOpen = () => invoke<void>("bin_open");
export const binEmpty = () => invoke<void>("bin_empty");
export const clipCount = () => sample(() => invoke<number>("clip_count"), 6);
export type NoteColor = "sun" | "ember" | "teal" | "rose" | "lilac";
export type Note = { id: string; text: string; color: NoteColor; locked: boolean; top: boolean };
export const notesCount = () => sample(() => invoke<number>("notes_count"), 3);
export const noteGet = (id: string) => invoke<Note | null>("note_get", { id });
export const noteNew = () => invoke<void>("note_new");
export const noteSet = (id: string, text: string, color: NoteColor) => invoke<void>("note_set", { id, text, color });
export const noteFlags = (id: string, locked: boolean, top: boolean) => invoke<void>("note_flags", { id, locked, top });
export const noteDelete = (id: string) => invoke<void>("note_delete", { id });
export const backupExport = () => invoke<string>("backup_export");
export const backupImport = () => invoke<string>("backup_import");
export type UpdateInfo = { current: string; latest: string; available: boolean; notes: string; size: number; checked: boolean; error: string; canInstall: boolean };
export const updateState = () => invoke<UpdateInfo>("update_state");
export const updateCheck = () => invoke<UpdateInfo>("update_check");
export const updateInstall = () => invoke<void>("update_install");
export const setAutoUpdate = (check: boolean, install: boolean) => invoke<void>("set_auto_update", { check, install });
export const langSelect = (hkl: string) => invoke<void>("lang_select", { hkl });
export const langToggle = (iconCx: number, rows: number, wasOpen: boolean) => invoke<void>("lang_pop_toggle", { iconCx, rows, wasOpen });
export const langGet = () => sample(() => invoke<Lang>("lang_get"), { code: "TR", name: "Türkçe" });
export const langSwitch = () => invoke<void>("lang_switch");
export type Battery = { percent: number; charging: boolean; plugged: boolean };
export const sensorsGet = () =>
  sample(() => invoke<Sensors>("sensors_get"), {
    cpu: { load: 19, temp: 63, power: 54 },
    gpu: { load: 6, temp: 43, power: 40 },
    ram: { used: 22.4e9, total: 33.4e9 },
    vram: { used: 3.0e9, total: 17.1e9 },
    down: 1.4e6,
    up: 5e4,
    disks: [
      { name: "SSD 1", temp: 41, read: 2.1, write: 0.4 },
      { name: "SSD 2", temp: 38, read: 0, write: 0 },
    ],
    hwinfo: true,
  });
export const batteryGet = () => sample(() => invoke<Battery | null>("battery_get"), { percent: 76, charging: true, plugged: true } as Battery | null);
export type TopRow = { name: string; icon: string; value: number };
export type Top = { cpu: TopRow[]; ram: TopRow[] };
export const topGet = () =>
  sample(() => invoke<Top>("top_get"), {
    cpu: [{ name: "Tarayıcı", icon: "", value: 4.4 }, { name: "Oyun", icon: "", value: 2.2 }, { name: "Kod düzenleyici", icon: "", value: 0.9 }],
    ram: [{ name: "Tarayıcı", icon: "", value: 1.7e9 }, { name: "Oyun", icon: "", value: 7e8 }, { name: "Kod düzenleyici", icon: "", value: 6e8 }],
  });
export const openTaskManager = () => invoke<void>("open_task_manager");
export type ScreenTime = { total: number; apps: { name: string; secs: number }[] };
export const screentimeGet = () => sample(() => invoke<ScreenTime>("screentime_get"), { total: 2640, apps: [] });
