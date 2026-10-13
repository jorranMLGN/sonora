import native from '../modules/sonora-core';

/** The switches the app keeps for itself. */
export type Flag = 'introSeen';

/** Where the switches live without the native module: for this run only. */
const memory = new Map<Flag, boolean>();

/** Where the string settings live without the native module: for this run only. */
const strings = new Map<string, string>();

/** Whether `flag` is on. Off until it is set, on a fresh install too. */
export function flag(name: Flag): boolean {
  return native === null ? (memory.get(name) ?? false) : native.flag(name);
}

export function setFlag(name: Flag, on: boolean): void {
  if (native === null) memory.set(name, on);
  else native.setFlag(name, on);
}

/**
 * Whether the native module can keep string settings. A development client built before it
 * could falls back to memory, like Expo Go.
 */
function stored(): boolean {
  return native !== null && typeof native.preference === 'function';
}

/** The string kept under `key`, or null while it was never set. */
export function preference(key: string): string | null {
  return stored() ? native!.preference(key) : (strings.get(key) ?? null);
}

/** Keeps `value` under `key`; null forgets it, so it reads as never set. */
export function setPreference(key: string, value: string | null): void {
  if (stored()) native!.setPreference(key, value);
  else if (value === null) strings.delete(key);
  else strings.set(key, value);
}
