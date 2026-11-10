// Turning numbers and times into text. UK English, binary units to match
// what File Explorer shows.

const UNITS = ["B", "KB", "MB", "GB", "TB"];

/** 1536 -> "1.5 KB". Whole numbers for bytes, and once past 100 of a unit. */
export function bytes(n) {
  let value = Math.max(0, Number(n) || 0);
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  const digits = unit === 0 || value >= 100 ? 0 : 1;
  return `${value.toFixed(digits)} ${UNITS[unit]}`;
}

/** Split a size into its number and unit, for big headline figures. */
export function bytesParts(n) {
  const [value, unit] = bytes(n).split(" ");
  return { value, unit };
}

const countFormat = new Intl.NumberFormat("en-GB");

/** 12345 -> "12,345" */
export function count(n) {
  return countFormat.format(Number(n) || 0);
}

/** plural(1, "file") -> "1 file"; plural(3, "file") -> "3 files" */
export function plural(n, one, many = one + "s") {
  return `${count(n)} ${Number(n) === 1 ? one : many}`;
}

const dateFormat = new Intl.DateTimeFormat("en-GB", { day: "numeric", month: "short", year: "numeric" });
const timeFormat = new Intl.DateTimeFormat("en-GB", { hour: "2-digit", minute: "2-digit" });

function sameDay(a, b) {
  return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
}

/** Unix seconds -> "3 Nov 2026". Empty string for nothing. */
export function date(unixSecs) {
  if (!unixSecs) return "";
  return dateFormat.format(new Date(unixSecs * 1000));
}

/** Unix seconds -> "today at 14:02", "yesterday at 09:15", "3 Nov 2026 at 14:02". */
export function when(unixSecs) {
  if (!unixSecs) return "";
  const d = new Date(unixSecs * 1000);
  const now = new Date();
  const yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1);
  const time = timeFormat.format(d);
  if (sameDay(d, now)) return `today at ${time}`;
  if (sameDay(d, yesterday)) return `yesterday at ${time}`;
  return `${dateFormat.format(d)} at ${time}`;
}

/** 5400 -> "1 hour 30 minutes" */
export function duration(secs) {
  const total = Math.max(0, Math.floor(secs));
  const days = Math.floor(total / 86400);
  const hours = Math.floor((total % 86400) / 3600);
  const mins = Math.floor((total % 3600) / 60);
  if (days > 0) return `${plural(days, "day")} ${plural(hours, "hour")}`;
  if (hours > 0) return `${plural(hours, "hour")} ${plural(mins, "minute")}`;
  return plural(Math.max(1, mins), "minute");
}

/** 6100 -> "6 seconds"; 250 -> "under a second" */
export function elapsed(ms) {
  if (ms < 1000) return "under a second";
  const secs = Math.round(ms / 1000);
  if (secs < 90) return plural(secs, "second");
  return plural(Math.round(secs / 60), "minute");
}

/** 0.4567 -> "46%" */
export function percent(fraction) {
  return `${Math.round(fraction * 100)}%`;
}

/** Shorten a long path in the middle so the drive and file name both show. */
export function midPath(path, max = 64) {
  if (path.length <= max) return path;
  const keep = max - 1;
  const head = Math.ceil(keep * 0.4);
  const tail = keep - head;
  return path.slice(0, head) + "…" + path.slice(path.length - tail);
}

const STATUS_WORDS = { good: "Good", attention: "Needs attention", problem: "Problem", unknown: "Not checked" };

export function statusWord(status) {
  return STATUS_WORDS[status] || STATUS_WORDS.unknown;
}
