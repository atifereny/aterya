import { EVENTS, useEvent, type Weather as WeatherData } from "../lib/ipc";
import { weatherLabel } from "../lib/weatherCodes";

export function Weather() {
  const w = useEvent<WeatherData | null>(EVENTS.weather, null);
  if (!w)
    return (
      <div className="tile weather">
        <span className="dim">bilinmiyor</span>
      </div>
    );
  const { label, icon } = weatherLabel(w.code);
  return (
    <div className="tile weather" title={`${w.city} · ${label}`}>
      <span className="big">{Math.round(w.tempC)}°</span>
      <span className="dim">
        {w.city} · {icon} {label}
      </span>
    </div>
  );
}
