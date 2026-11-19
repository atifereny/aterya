import type { ReactNode } from "react";

/** Resizes a widget (layout included, unlike a transform). The wrapper is always there: toggling it would remount the widget, and a remounted widget stays empty until the backend sends its next event. */
export function Zoom({ scale, children }: { scale?: number; children: ReactNode }) {
  return <div className="wz" style={scale && scale !== 1 ? { zoom: scale } : undefined}>{children}</div>;
}
