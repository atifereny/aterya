// WMO weather codes (Open-Meteo) -> Turkish label + emoji icon.
const RANGES: [number, number, string, string][] = [
  [0, 0, "Açık", "☀️"],
  [1, 3, "Parçalı bulutlu", "⛅"],
  [45, 48, "Sisli", "🌫️"],
  [51, 57, "Çisenti", "🌦️"],
  [61, 65, "Yağmurlu", "🌧️"],
  [66, 67, "Dondurucu yağmur", "🌧️"],
  [71, 77, "Karlı", "❄️"],
  [80, 82, "Sağanak", "🌧️"],
  [85, 86, "Kar sağanağı", "🌨️"],
  [95, 99, "Fırtınalı", "⛈️"],
];

export function weatherLabel(code: number): { label: string; icon: string } {
  const r = RANGES.find(([lo, hi]) => code >= lo && code <= hi);
  return r ? { label: r[2], icon: r[3] } : { label: "bilinmiyor", icon: "❔" };
}
