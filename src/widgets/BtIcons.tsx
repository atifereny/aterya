import type { ReactNode } from "react";

const base = { viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 1.8, strokeLinecap: "round", strokeLinejoin: "round" } as const;

/** Icons a Bluetooth device can have; the key is what the settings store. */
export const BT_ICONS: Record<string, { label: string; draw: ReactNode }> = {
  bluetooth: { label: "Bluetooth", draw: <path d="M7 7l10 10-5 5V2l5 5L7 17" /> },
  headphones: { label: "Kulaklık", draw: <><path d="M4 15v-3a8 8 0 0116 0v3" /><rect x="3" y="14" width="4" height="6" rx="1.6" /><rect x="17" y="14" width="4" height="6" rx="1.6" /></> },
  earbuds: { label: "Kulakiçi", draw: <><path d="M8 3a3 3 0 00-3 3v4a3 3 0 006 0V6a3 3 0 00-3-3z" /><path d="M8 13v6a2 2 0 004 0" /><path d="M17 8a3 3 0 013 3v2a3 3 0 01-6 0v-2a3 3 0 013-3z" /></> },
  keyboard: { label: "Klavye", draw: <><rect x="2.5" y="6" width="19" height="12" rx="2.5" /><path d="M6 10h.01M10 10h.01M14 10h.01M18 10h.01M7 14h10" /></> },
  mouse: { label: "Fare", draw: <><rect x="6.5" y="3" width="11" height="18" rx="5.5" /><path d="M12 3v6" /></> },
  gamepad: { label: "Kol", draw: <><path d="M7 7h10a4 4 0 014 4v2a3.5 3.5 0 01-6 2.5L14 14h-4l-1 1.5A3.5 3.5 0 013 13v-2a4 4 0 014-4z" /><path d="M8 10v3M6.5 11.5h3M16 11h.01M18 12.5h.01" /></> },
  speaker: { label: "Hoparlör", draw: <><rect x="6" y="2.5" width="12" height="19" rx="3" /><circle cx="12" cy="14.5" r="3" /><circle cx="12" cy="7.5" r="1" /></> },
  phone: { label: "Telefon", draw: <><rect x="7" y="2.5" width="10" height="19" rx="2.5" /><path d="M11 18.5h2" /></> },
  watch: { label: "Saat", draw: <><rect x="7" y="6.5" width="10" height="11" rx="3" /><path d="M9 6.5l.7-3.5h4.6l.7 3.5M9 17.5l.7 3.5h4.6l.7-3.5M12 10v2.5l1.5 1" /></> },
};

export function BtIcon({ kind, size = 16 }: { kind: string; size?: number }) {
  const d = BT_ICONS[kind] ?? BT_ICONS.bluetooth;
  return (
    <svg {...base} width={size} height={size} aria-hidden>
      {d.draw}
    </svg>
  );
}

export const batColor = (p: number) => (p > 50 ? "#4ade80" : p >= 20 ? "#fbbf24" : "#f87171");
