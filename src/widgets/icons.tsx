// Line icons (currentColor) for the system controls.
const base = { width: 18, height: 18, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 1.8, strokeLinecap: "round", strokeLinejoin: "round" } as const;

export function SpeakerIcon({ level, size = 18 }: { level: "mute" | "low" | "high"; size?: number }) {
  return (
    <svg {...base} width={size} height={size}>
      <path d="M11 5.2 6.6 8.9H3.5v6.2h3.1L11 18.8V5.2z" />
      {level === "mute" ? (
        <path d="M15.5 9.5l5 5M20.5 9.5l-5 5" />
      ) : (
        <>
          <path d="M15.3 9.3a3.8 3.8 0 010 5.4" />
          {level === "high" && <path d="M18.3 6.6a7.6 7.6 0 010 10.8" />}
        </>
      )}
    </svg>
  );
}

export function NetworkIcon({ kind }: { kind: "ethernet" | "wifi" | "offline" }) {
  if (kind === "wifi")
    return (
      <svg {...base}>
        <path d="M2.5 9a14 14 0 0119 0M5.5 12.5a9.5 9.5 0 0113 0M8.7 16a5 5 0 016.6 0" />
        <circle cx="12" cy="19.2" r="1.1" fill="currentColor" stroke="none" />
      </svg>
    );
  if (kind === "offline")
    return (
      <svg {...base}>
        <path d="M2.5 9a14 14 0 0119 0M5.5 12.5a9.5 9.5 0 0113 0M8.7 16a5 5 0 016.6 0" opacity="0.55" />
        <path d="M3.5 3.5l17 17" strokeWidth="2.2" />
      </svg>
    );
  // Ethernet: a wired network (one node feeding two).
  return (
    <svg {...base}>
      <rect x="8.5" y="3" width="7" height="5.5" rx="1.2" />
      <rect x="2.5" y="15.5" width="7" height="5.5" rx="1.2" />
      <rect x="14.5" y="15.5" width="7" height="5.5" rx="1.2" />
      <path d="M12 8.5v3.5M6 15.5V12h12v3.5" />
    </svg>
  );
}
