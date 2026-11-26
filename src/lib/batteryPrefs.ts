/** The built-in devices of the battery widget. */
export const BUILTIN_BATTERIES = ["mouse", "headset", "keyboard"];

/** Key of a Bluetooth device in the battery choices. */
export const btKey = (name: string) => `bt:${name}`;

/** Is the device shown in the battery widget? Built-ins unless turned off; Bluetooth devices only once turned on. */
export function batteryShown(prefs: Record<string, boolean>, id: string): boolean {
  return prefs[id] ?? BUILTIN_BATTERIES.includes(id);
}

/** A Bluetooth device the user has not decided on yet. */
export const batteryUndecided = (prefs: Record<string, boolean>, id: string) => !(id in prefs) && !BUILTIN_BATTERIES.includes(id);
