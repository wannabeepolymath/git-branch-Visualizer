/** Compact relative time for unix-seconds timestamps: "now", "5m", "3h", "2d", "4mo", "1y". */
export function relTime(unixSecs: number): string {
  const s = Math.max(0, Math.floor(Date.now() / 1000) - unixSecs);
  if (s < 45) return "now";
  const m = Math.round(s / 60);
  if (m < 60) return `${m}m`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h}h`;
  const d = Math.round(h / 24);
  if (d < 30) return `${d}d`;
  const mo = Math.round(d / 30);
  if (mo < 12) return `${mo}mo`;
  return `${Math.round(d / 365)}y`;
}
