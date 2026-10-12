/** Display formatting for the shell. Local time zone unless a name says UTC; data stays UTC epoch ms. */

const day = new Intl.DateTimeFormat(undefined, { day: "numeric", month: "short" });
const date = new Intl.DateTimeFormat(undefined, { day: "numeric", month: "short", year: "numeric" });
const dateTime = new Intl.DateTimeFormat(undefined, { day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit" });
const time = new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit", second: "2-digit" });
const rel = new Intl.RelativeTimeFormat(undefined, { numeric: "auto", style: "short" });
const month = new Intl.DateTimeFormat(undefined, { month: "short" });
// for panels that sit beside UTC day bins (conversation activity): one zone, named
const utcDay = new Intl.DateTimeFormat(undefined, { day: "numeric", month: "short", timeZone: "UTC" });
const utcDate = new Intl.DateTimeFormat(undefined, { day: "numeric", month: "short", year: "numeric", timeZone: "UTC" });
const utcDateTime = new Intl.DateTimeFormat(undefined, { day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit", timeZone: "UTC", timeZoneName: "short" });

export const fmt = {
  day: (ms: number) => day.format(ms),
  date: (ms: number) => date.format(ms),
  dateTime: (ms: number) => dateTime.format(ms),
  time: (ms: number) => time.format(ms),
  month: (ms: number) => month.format(ms),
  utcDay: (ms: number) => utcDay.format(ms),
  utcDate: (ms: number) => utcDate.format(ms),
  /** carries its zone name: "13 Jun 2026, 19:59 UTC" */
  utcDateTime: (ms: number) => utcDateTime.format(ms),
  count: (n: number) => n.toLocaleString(),
  /** "3 min ago", "yesterday", "in 2 days" */
  ago(ms: number, now = Date.now()): string {
    const s = (ms - now) / 1000;
    const a = Math.abs(s);
    if (a < 45) return "just now";
    if (a < 3600) return rel.format(Math.round(s / 60), "minute");
    if (a < 86_400) return rel.format(Math.round(s / 3600), "hour");
    if (a < 86_400 * 30) return rel.format(Math.round(s / 86_400), "day");
    return rel.format(Math.round(s / (86_400 * 30)), "month");
  },
  /** "4 d 3 h", "12 min": a span of time */
  span(ms: number): string {
    const m = Math.round(ms / 60_000);
    if (m < 60) return `${m} min`;
    const h = Math.floor(m / 60);
    if (h < 48) return `${h} h ${m % 60} min`;
    return `${Math.floor(h / 24)} d ${h % 24} h`;
  },
};

/** "20260801T000000Z" → epoch ms (snapshot ids are export timestamps). */
export function snapshotIdMs(id: string): number | null {
  const m = /^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})Z$/.exec(id);
  return m ? Date.UTC(+m[1], +m[2] - 1, +m[3], +m[4], +m[5], +m[6]) : null;
}
