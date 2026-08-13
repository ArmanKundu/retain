// Formatting helpers.

/** `M:SS` under an hour, `H:MM:SS` beyond. Matches the menu bar exactly. */
export function clock(totalSeconds: number): string {
  const s = Math.max(0, Math.floor(totalSeconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  return h > 0
    ? `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`
    : `${m}:${String(sec).padStart(2, "0")}`;
}

/** Human duration for summaries: "1h 24m", "45m", "under a minute". */
export function duration(totalSeconds: number): string {
  const mins = Math.floor(Math.max(0, totalSeconds) / 60);
  if (mins < 1) return "under a minute";
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  if (h === 0) return `${m}m`;
  if (m === 0) return `${h}h`;
  return `${h}h ${m}m`;
}

export function hoursLabel(minutes: number): string {
  const h = minutes / 60;
  return h >= 10 || Number.isInteger(h) ? `${Math.round(h)}h` : `${h.toFixed(1)}h`;
}

/** Local 'YYYY-MM-DD'. Must stay local — see the note in src-tauri/src/util.rs. */
export function localDate(d: Date = new Date()): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

export function addDays(d: Date, days: number): Date {
  const copy = new Date(d);
  copy.setDate(copy.getDate() + days);
  return copy;
}

const DAY_NAMES = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
export const WEEKDAY_LABELS = DAY_NAMES;
export const WEEKDAY_SHORT = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/** Monday-based weekday index, matching chrono's `num_days_from_monday`. */
export function mondayIndex(d: Date): number {
  return (d.getDay() + 6) % 7;
}

export function prettyDate(iso: string): string {
  const [y, m, d] = iso.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString(undefined, {
    weekday: "long",
    day: "numeric",
    month: "long",
  });
}

export function timeOfDay(rfc3339: string): string {
  return new Date(rfc3339).toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });
}

/** Time-appropriate greeting. Used with the name from onboarding. */
export function greeting(name: string): string {
  const h = new Date().getHours();
  const part = h < 12 ? "Morning" : h < 18 ? "Afternoon" : "Evening";
  return name ? `${part}, ${name}` : part;
}
