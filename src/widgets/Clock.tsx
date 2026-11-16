import { useEffect, useState } from "react";
import { formatClock, msUntilNextMinute } from "../lib/clock";
import { openNotifications } from "../lib/ipc";

export function Clock() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    let t: ReturnType<typeof setTimeout>;
    const tick = () => {
      const d = new Date();
      setNow(d);
      t = setTimeout(tick, msUntilNextMinute(d));
    };
    t = setTimeout(tick, msUntilNextMinute(new Date()));
    return () => clearTimeout(t);
  }, []);
  const { time, date } = formatClock(now);
  return (
    <div className="tile clock" onClick={() => void openNotifications()}>
      <span className="big">{time}</span>
      <span className="dim">{date}</span>
    </div>
  );
}
