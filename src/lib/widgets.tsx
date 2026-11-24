import type { ReactNode } from "react";
import type { WidgetId } from "./ipc";
import { NowPlaying } from "../widgets/NowPlaying";
import { Weather } from "../widgets/Weather";
import { ClaudeUsage } from "../widgets/ClaudeUsage";
import { Batteries } from "../widgets/Batteries";
import { Tray } from "../widgets/Tray";
import { SystemControls } from "../widgets/SystemControls";
import { Clock } from "../widgets/Clock";
import { ScreenTime } from "../widgets/ScreenTime";
import { TopCpu, TopRam } from "../widgets/TopApps";
import { Brightness } from "../widgets/Brightness";
import { Notifications } from "../widgets/Notifications";
import { ShowDesktop } from "../widgets/ShowDesktop";
import { BatteryRing, CpuRing, GpuRing } from "../widgets/Rings";
import { KeyboardLang, MemoryUsage, NetSpeed, Storage } from "../widgets/Usage";
import { Bluetooth, SideBluetooth } from "../widgets/Bluetooth";
import { Calendar, SideCalendar } from "../widgets/Calendar";
import { Clipboard, Microphone, RecycleBin, StickyNotes } from "../widgets/Extras";
import { SideBrightness, SideClaude, SideClock, SideMemory, SideNetSpeed, SideNowPlaying, SideScreenTime, SideWeather } from "../widgets/Side";

/** Widgets the dock can show. Add a new widget here and in `WIDGETS` (config.rs). */
export const WIDGET_INFO: Record<WidgetId, { label: string; desc: string; category: string; group: "apps" | "info" | "system"; render: () => ReactNode; side?: () => ReactNode }> = {
  // Rendered by App.tsx (it needs the free width); render is unused.
  apps: { label: "Başlat ve uygulamalar", desc: "Başlat düğmesi, sabitlenmiş ve açık uygulamalar (klasik görev çubuğu).", category: "Uygulamalar ve dosyalar", group: "apps", render: () => null },
  nowplaying: { label: "Şu an çalan", desc: "Kapak, parça adı ve oynat/duraklat/geç tuşları.", category: "Medya ve ses", group: "info", render: () => <NowPlaying />, side: () => <SideNowPlaying /> },
  weather: { label: "Hava durumu", desc: "Şehrin anlık sıcaklığı ve durumu.", category: "Hava durumu", group: "info", render: () => <Weather />, side: () => <SideWeather /> },
  claude: { label: "Claude kullanımı", desc: "5 saatlik ve haftalık Claude kullanım limiti.", category: "Yapay zekâ", group: "info", render: () => <ClaudeUsage />, side: () => <SideClaude /> },
  batteries: { label: "Cihaz pilleri", desc: "Fare, kulaklık ve klavye şarj seviyeleri.", category: "Sistem", group: "info", render: () => <Batteries /> },
  tray: { label: "Gizli simgeler", desc: "Windows'un gizli simge panelini açar.", category: "Sistem", group: "system", render: () => <Tray /> },
  system: { label: "Ses ve ağ", desc: "Ses karıştırıcı ve ağ durumu.", category: "Medya ve ses", group: "system", render: () => <SystemControls /> },
  clock: { label: "Saat ve tarih", desc: "Saat, tarih ve bildirim/takvim paneli.", category: "Saatler", group: "system", render: () => <Clock />, side: () => <SideClock /> },
  screentime: { label: "Ekran süresi", desc: "Bugün bilgisayar başında geçen süre; üzerine gelince en çok kullanılan uygulamalar.", category: "Üretkenlik", group: "info", render: () => <ScreenTime />, side: () => <SideScreenTime /> },
  topcpu: { label: "En çok işlemci", desc: "İşlemciyi en çok kullanan üç uygulama; tıklayınca Görev Yöneticisi açılır.", category: "Sistem", group: "info", render: () => <TopCpu /> },
  topram: { label: "En çok bellek", desc: "Belleği en çok tutan üç uygulama; Görev Yöneticisi gibi işlemleri toplar.", category: "Sistem", group: "info", render: () => <TopRam /> },
  brightness: { label: "Parlaklık", desc: "Tüm ekranların parlaklığını tek kaydırıcıyla ayarlar.", category: "Sistem", group: "system", render: () => <Brightness />, side: () => <SideBrightness /> },
  notifications: { label: "Bildirimler", desc: "Bildirim merkezini dock'tan açar.", category: "Sistem", group: "system", render: () => <Notifications /> },
  showdesktop: { label: "Masaüstünü göster", desc: "Tüm pencereleri gizleyip masaüstünü gösterir; tekrar basınca geri getirir.", category: "Sistem", group: "system", render: () => <ShowDesktop /> },
  batteryring: { label: "Pil halkası", desc: "Dizüstü pilinin şarjı halka olarak; şarj olurken halkada bir kıvılcım döner.", category: "Sistem", group: "info", render: () => <BatteryRing /> },
  cpuring: { label: "CPU halkası", desc: "İşlemci yükü halkada, sıcaklık ortada, yanında yüzde ve watt. Sıcaklık/watt için HWiNFO Shared Memory gerekir.", category: "Sistem", group: "info", render: () => <CpuRing /> },
  gpuring: { label: "GPU halkası", desc: "Ekran kartı yükü halkada, sıcaklık ortada, yanında yüzde ve watt (NVIDIA).", category: "Sistem", group: "info", render: () => <GpuRing /> },
  memory: { label: "RAM ve VRAM", desc: "Kullanılan RAM ve ekran kartı belleği, yüzde ve GB olarak.", category: "Sistem", group: "info", render: () => <MemoryUsage />, side: () => <SideMemory /> },
  storage: { label: "SSD'ler", desc: "Her diskin sıcaklığı ve o anki okuma/yazma hızı. HWiNFO Shared Memory gerekir.", category: "Sistem", group: "info", render: () => <Storage /> },
  netspeed: { label: "İnternet hızı", desc: "Anlık indirme ve yükleme hızı.", category: "Sistem", group: "info", render: () => <NetSpeed />, side: () => <SideNetSpeed /> },
  keyboardlang: { label: "Klavye dili", desc: "Aktif klavye dili (TR, EN…). Alt+Shift ile değişince kendiliğinden güncellenir; tıklayınca dil değişir.", category: "Sistem", group: "system", render: () => <KeyboardLang /> },
  microphone: { label: "Mikrofon", desc: "Mikrofonu tek tıkla kapatır ve açar. Kapalıyken simge üstü çizili görünür.", category: "Medya ve ses", group: "system", render: () => <Microphone /> },
  recyclebin: { label: "Geri Dönüşüm Kutusu", desc: "Kutuda kaç öğe olduğunu gösterir; tıklayınca açılır, sağ tıklayınca boşaltır.", category: "Sistem", group: "system", render: () => <RecycleBin /> },
  stickynotes: { label: "Yapışkan notlar", desc: "Tıklayınca masaüstüne yeni bir yapışkan not koyar. Notlar yerinde kalır, taşınır ve yeniden boyutlandırılır.", category: "Üretkenlik", group: "info", render: () => <StickyNotes /> },
  clipboard: { label: "Pano geçmişi", desc: "Son kopyaladığın metinler. Tıklayınca liste açılır, birine basınca tekrar panoya alınır. Geçmiş sadece bellekte tutulur, diske yazılmaz; parola yöneticilerinin kopyaları kaydedilmez.", category: "Üretkenlik", group: "info", render: () => <Clipboard /> },
  bluetooth: { label: "Bluetooth", desc: "Bluetooth'u açıp kapatır; seçtiğin cihazların simgesini ve şarjını gösterir. Tıklayınca bağlı cihazlar, şarj seviyeleri ve simge seçimi açılır.", category: "Sistem", group: "system", render: () => <Bluetooth />, side: () => <SideBluetooth /> },
  calendar: { label: "Takvim", desc: "Bugünün tarihi ve sıradaki etkinlik. Tıklayınca ay takvimi açılır; Google Takvim, Outlook ve iCloud takvimlerini (iCal adresiyle) ve kendi etkinliklerini gösterir.", category: "Üretkenlik", group: "info", render: () => <Calendar />, side: () => <SideCalendar /> },
};
