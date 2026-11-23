/** A dock on the left or right screen edge runs top to bottom. The edge is mirrored on <html data-edge>. */
export const isVertical = () => ["left", "right"].includes(document.documentElement.dataset.edge ?? "");

/** An element's center along the dock's own axis (y for a side dock, x otherwise): what the popups center on. */
export const along = (r: DOMRect) => (isVertical() ? r.top + r.height / 2 : r.left + r.width / 2);
