import type { components } from "../api/schema";

export type Occurrence = components["schemas"]["Occurrence"];
export type CalendarEvent = components["schemas"]["CalendarEvent"];

export const DAY_MS = 86_400_000;
/** The part of the day "free hours" counts: 07:00 to 22:00. */
export const AWAKE_FROM = 7;
export const AWAKE_TO = 22;

export const DAY_NAMES = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] as const;
export const WEEKDAYS = 31;
export const WEEKENDS = 96;

/** The browser's IANA time zone, which new events are planned in. */
export function browserZone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
  } catch {
    return "UTC";
  }
}

/** Local midnight on the Monday of `d`'s week. */
export function weekStart(d: Date): Date {
  const day = new Date(d.getFullYear(), d.getMonth(), d.getDate());
  const offset = (day.getDay() + 6) % 7;
  return addDays(day, -offset);
}

/** Calendar days, safe across clock changes. */
export function addDays(d: Date, n: number): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() + n, d.getHours(), d.getMinutes());
}

const pad = (n: number) => String(n).padStart(2, "0");

/** `2026-11-14`, local. */
export function dayKey(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** `13:00`, local. */
export function clock(d: Date): string {
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** `2026-11-14T13:00`: what the server calls a local start. */
export function localStart(d: Date): string {
  return `${dayKey(d)}T${clock(d)}`;
}

/** A `YYYY-MM-DD` and `HH:MM` as a local Date. */
export function fromParts(day: string, time: string): Date {
  const [y, m, d] = day.split("-").map(Number) as [number, number, number];
  const [h, min] = time.split(":").map(Number) as [number, number];
  return new Date(y, m - 1, d, h, min);
}

/** Minutes after local midnight of `day`. Can be negative or past 1440. */
export function minutesInto(day: Date, at: Date): number {
  return Math.round((at.getTime() - day.getTime()) / 60_000);
}

/** Rounds to the nearest `step` minutes. */
export function snap(minutes: number, step = 15): number {
  return Math.round(minutes / step) * step;
}

/** "4h", "1h 30", "45m". */
export function hoursText(minutes: number): string {
  const h = Math.floor(minutes / 60);
  const m = Math.round(minutes % 60);
  if (h === 0) return `${m}m`;
  return m === 0 ? `${h}h` : `${h}h ${pad(m)}`;
}

/** "weekdays", "weekends", "every day", "Mon, Thu" or "" for once. */
export function repeatText(mask: number): string {
  if (mask === 0) return "";
  if (mask === 127) return "every day";
  if (mask === WEEKDAYS) return "weekdays";
  if (mask === WEEKENDS) return "weekends";
  return DAY_NAMES.filter((_, i) => mask & (1 << i)).join(", ");
}

/** Bit for a date's weekday, Monday = bit 0. */
export function dayBit(d: Date): number {
  return 1 << ((d.getDay() + 6) % 7);
}

/** "Week of 9 November", or "This week" style label parts. */
export function weekLabel(monday: Date): string {
  const sunday = addDays(monday, 6);
  const sameMonth = monday.getMonth() === sunday.getMonth();
  const first = monday.toLocaleDateString("en-GB", { day: "numeric", ...(sameMonth ? {} : { month: "short" }) });
  const last = sunday.toLocaleDateString("en-GB", { day: "numeric", month: "short" });
  return `${first} – ${last}`;
}

/** "Sat 14 Nov · 13:00–17:00". */
export function whenText(o: Pick<Occurrence, "start_at" | "end_at">): string {
  const s = new Date(o.start_at);
  const e = new Date(o.end_at);
  const day = s.toLocaleDateString("en-GB", { weekday: "short", day: "numeric", month: "short" });
  return `${day} · ${clock(s)}–${clock(e)}`;
}

/** What an occurrence is called on its block. */
export function eventLabel(e: CalendarEvent): string {
  if (e.kind === "busy") return e.title || "Busy";
  return e.title || e.project_title || "Session";
}

/** You're on it: you planned it or said you'd join. */
export function isMine(o: Occurrence, me: string): boolean {
  return o.event.user_id === me || o.joined;
}

export interface Placed {
  occ: Occurrence;
  /** Minutes after the day's midnight, clipped to the day. */
  top: number;
  bottom: number;
  /** Side-by-side lane when blocks overlap, and how many lanes that cluster has. */
  lane: number;
  lanes: number;
}

/**
 * Lays out one day's blocks: each clipped to the day, overlapping ones side
 * by side. Busy time and sessions share the lanes.
 */
export function placeDay(day: Date, list: Occurrence[]): Placed[] {
  const next = addDays(day, 1);
  const items = list
    .map((occ) => {
      const s = new Date(occ.start_at);
      const e = new Date(occ.end_at);
      return { occ, s, e };
    })
    .filter(({ s, e }) => s < next && e > day)
    .map(({ occ, s, e }) => ({
      occ,
      top: Math.max(0, minutesInto(day, s)),
      bottom: Math.min(minutesInto(day, next), minutesInto(day, e)),
      lane: 0,
      lanes: 1,
    }))
    .sort((a, b) => a.top - b.top || b.bottom - a.bottom);

  const out: Placed[] = [];
  let cluster: Placed[] = [];
  let clusterEnd = -1;
  const close = () => {
    const lanes = Math.max(1, ...cluster.map((c) => c.lane + 1));
    for (const c of cluster) c.lanes = lanes;
    out.push(...cluster);
    cluster = [];
  };
  for (const item of items) {
    if (item.top >= clusterEnd && cluster.length) close();
    const used = new Set(cluster.filter((c) => c.bottom > item.top).map((c) => c.lane));
    let lane = 0;
    while (used.has(lane)) lane++;
    item.lane = lane;
    cluster.push(item);
    clusterEnd = Math.max(clusterEnd, item.bottom);
  }
  if (cluster.length) close();
  return out;
}

/** Minutes covered by a set of intervals, overlaps counted once. */
function unionMinutes(ranges: [number, number][]): number {
  const sorted = ranges.filter(([a, b]) => b > a).sort((x, y) => x[0] - y[0]);
  let total = 0;
  let end = -Infinity;
  let start = 0;
  for (const [a, b] of sorted) {
    if (a > end) {
      if (end > -Infinity) total += end - start;
      start = a;
      end = b;
    } else end = Math.max(end, b);
  }
  if (end > -Infinity) total += end - start;
  return total;
}

export interface WeekSummary {
  /** Minutes of sessions you planned or are joining. */
  planned: number;
  /** Minutes between 07:00 and 22:00 not taken by busy time or your sessions. */
  free: number;
  /** Your session minutes per project, most first. */
  perProject: { id: string; title: string; color: string; minutes: number }[];
}

export function summarise(monday: Date, list: Occurrence[], me: string): WeekSummary {
  const mine = list.filter((o) => o.event.kind === "busy" || isMine(o, me));
  let planned = 0;
  const per = new Map<string, WeekSummary["perProject"][number]>();
  for (const o of mine) {
    if (o.event.kind !== "session" || !o.event.project_id) continue;
    const m = (new Date(o.end_at).getTime() - new Date(o.start_at).getTime()) / 60_000;
    planned += m;
    const row = per.get(o.event.project_id) ?? {
      id: o.event.project_id,
      title: o.event.project_title ?? "Project",
      color: o.event.project_color ?? "slot-1",
      minutes: 0,
    };
    row.minutes += m;
    per.set(o.event.project_id, row);
  }
  let taken = 0;
  for (let i = 0; i < 7; i++) {
    const day = addDays(monday, i);
    const from = AWAKE_FROM * 60;
    const to = AWAKE_TO * 60;
    const ranges: [number, number][] = mine.map((o) => [
      Math.max(from, minutesInto(day, new Date(o.start_at))),
      Math.min(to, minutesInto(day, new Date(o.end_at))),
    ]);
    taken += unionMinutes(ranges);
  }
  const awake = (AWAKE_TO - AWAKE_FROM) * 60 * 7;
  return {
    planned,
    free: Math.max(0, awake - taken),
    perProject: [...per.values()].sort((a, b) => b.minutes - a.minutes),
  };
}

/** Two-letter initials for a person's name. */
export function initials(name: string): string {
  const parts = name.trim().split(/\s+/);
  const letters = parts.length > 1 ? parts[0]![0]! + parts.at(-1)![0]! : name.slice(0, 2);
  return letters.toUpperCase();
}

/** The same occurrence as it is in a fresher list (after a join, say), or as it was. */
export function live(o: Occurrence | null, list: Occurrence[]): Occurrence | null {
  if (!o) return null;
  return list.find((x) => x.event.id === o.event.id && x.occurrence === o.occurrence) ?? o;
}
