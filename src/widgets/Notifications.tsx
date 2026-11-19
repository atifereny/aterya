import { openNotifications } from "../lib/ipc";

export function Notifications() {
  return (
    <div className="tile notif" title="Bildirim merkezi" onClick={() => void openNotifications()}>
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
        <path d="M6 9a6 6 0 0112 0c0 5 2 6.5 2 6.5H4S6 14 6 9zM10 19a2 2 0 004 0" />
      </svg>
    </div>
  );
}
