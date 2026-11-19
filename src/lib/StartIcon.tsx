export type StartIconKind = "windows" | "classic" | "outline" | "rounded" | "aterya" | "grid" | "dots";

export const START_ICONS: { id: StartIconKind; name: string }[] = [
  { id: "windows", name: "Windows 11" },
  { id: "classic", name: "Klasik" },
  { id: "outline", name: "Çizgili" },
  { id: "rounded", name: "Yuvarlak" },
  { id: "aterya", name: "Aterya" },
  { id: "grid", name: "Izgara" },
  { id: "dots", name: "Noktalar" },
];

/** The start button's glyph, drawn on a 16 x 16 grid in the current text color. */
export function StartIcon({ kind, size }: { kind: StartIconKind; size: number }) {
  const props = { width: size, height: size, viewBox: "0 0 16 16", "aria-hidden": true } as const;
  switch (kind) {
    case "classic":
      return (
        <svg {...props} fill="currentColor">
          <path d="M0 2.3 6.5 1.4v5.8H0zM7.4 1.3 16 0v7.2H7.4zM0 8.1h6.5v5.8L0 13zM7.4 8.1H16V16l-8.6-1.2z" />
        </svg>
      );
    case "outline":
      return (
        <svg {...props} fill="none" stroke="currentColor" strokeWidth="1.3">
          <rect x="1.6" y="1.6" width="5.6" height="5.6" rx="1" />
          <rect x="8.8" y="1.6" width="5.6" height="5.6" rx="1" />
          <rect x="1.6" y="8.8" width="5.6" height="5.6" rx="1" />
          <rect x="8.8" y="8.8" width="5.6" height="5.6" rx="1" />
        </svg>
      );
    case "rounded":
      return (
        <svg {...props} fill="currentColor">
          <rect x="1" y="1" width="6.2" height="6.2" rx="2.4" />
          <rect x="8.8" y="1" width="6.2" height="6.2" rx="2.4" />
          <rect x="1" y="8.8" width="6.2" height="6.2" rx="2.4" />
          <rect x="8.8" y="8.8" width="6.2" height="6.2" rx="2.4" />
        </svg>
      );
    case "aterya":
      return (
        <svg width={size} height={size} viewBox="0 0 84 84" aria-hidden="true">
          <defs>
            <linearGradient id="aterya-start-g" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0" stopColor="#ff7a1a" />
              <stop offset="1" stopColor="#0fb5ac" />
            </linearGradient>
          </defs>
          <rect width="84" height="84" rx="22" fill="url(#aterya-start-g)" />
          <path d="M24 62 L42 22 L60 62" fill="none" stroke="#fff" strokeWidth="6" strokeLinecap="round" strokeLinejoin="round" />
          <path d="M31 50 Q36.5 42 42 50 T53 50" fill="none" stroke="#fff" strokeWidth="3.5" strokeLinecap="round" />
        </svg>
      );
    case "grid":
      return (
        <svg {...props} fill="currentColor">
          {[1.5, 6.25, 11].flatMap((x) => [1.5, 6.25, 11].map((y) => <rect key={`${x}-${y}`} x={x} y={y} width="3.5" height="3.5" rx="0.9" />))}
        </svg>
      );
    case "dots":
      return (
        <svg {...props} fill="currentColor">
          <circle cx="4.5" cy="4.5" r="2.7" />
          <circle cx="11.5" cy="4.5" r="2.7" />
          <circle cx="4.5" cy="11.5" r="2.7" />
          <circle cx="11.5" cy="11.5" r="2.7" />
        </svg>
      );
    default:
      return (
        <svg {...props} fill="currentColor">
          <rect x="1" y="1" width="6.3" height="6.3" rx="0.8" />
          <rect x="8.7" y="1" width="6.3" height="6.3" rx="0.8" />
          <rect x="1" y="8.7" width="6.3" height="6.3" rx="0.8" />
          <rect x="8.7" y="8.7" width="6.3" height="6.3" rx="0.8" />
        </svg>
      );
  }
}
