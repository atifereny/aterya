import { showDesktop } from "../lib/ipc";

export function ShowDesktop() {
  return (
    <div className="tile notif" title="Masaüstünü göster" onClick={() => void showDesktop()}>
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
        <rect x="3" y="4" width="18" height="12" rx="2" />
        <path d="M8 20h8M12 16v4" />
      </svg>
    </div>
  );
}
