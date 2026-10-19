/** Formats milliseconds as `m:ss`. */
export function clock(ms: number): string {
  const whole = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}`;
}
