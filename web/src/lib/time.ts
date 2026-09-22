const rtf = new Intl.RelativeTimeFormat("de", { numeric: "auto", style: "short" });
const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 31536000],
  ["month", 2592000],
  ["week", 604800],
  ["day", 86400],
  ["hour", 3600],
  ["minute", 60],
];

export function ago(iso: string | null | undefined, now = Date.now()): string {
  if (!iso) return "–";
  const diff = (new Date(iso).getTime() - now) / 1000;
  for (const [unit, sec] of UNITS) {
    if (Math.abs(diff) >= sec) return rtf.format(Math.round(diff / sec), unit);
  }
  return "gerade eben";
}

export function dayLabel(iso: string): string {
  const d = new Date(iso);
  const today = new Date();
  const yesterday = new Date(Date.now() - 86400000);
  if (d.toDateString() === today.toDateString()) return "Heute";
  if (d.toDateString() === yesterday.toDateString()) return "Gestern";
  return d.toLocaleDateString("de-DE", { weekday: "long", day: "numeric", month: "long" });
}

export function clock(iso: string): string {
  return new Date(iso).toLocaleTimeString("de-DE", { hour: "2-digit", minute: "2-digit" });
}
