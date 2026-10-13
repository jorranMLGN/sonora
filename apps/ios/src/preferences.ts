import native from '../modules/sonora-core';

/** The switches the app keeps for itself. */
export type Flag = 'introSeen';

/** Where the switches live without the native module: for this run only. */
const memory = new Map<Flag, boolean>();

/** Whether `flag` is on. Off until it is set, on a fresh install too. */
export function flag(name: Flag): boolean {
  return native === null ? (memory.get(name) ?? false) : native.flag(name);
}

export function setFlag(name: Flag, on: boolean): void {
  if (native === null) memory.set(name, on);
  else native.setFlag(name, on);
}
