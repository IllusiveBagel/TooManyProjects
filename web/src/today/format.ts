import type { components } from "../api/schema";

export type Today = components["schemas"]["Today"];
export type SearchHit = components["schemas"]["SearchHit"];
export type QuickAddResult = components["schemas"]["QuickAddResult"];
export type LinkPreview = components["schemas"]["LinkPreview"];
export type QuickKind = "task" | "note" | "inbox";

const URL_RE = /\bhttps?:\/\/[^\s<>"')\]]+/i;

/** The first web address in some text, without trailing punctuation. */
export function findUrl(text: string): string | null {
  const m = URL_RE.exec(text);
  return m ? m[0].replace(/[.,;:!?]+$/, "") : null;
}

/** "printables.com/model/…" for showing a link in little space. */
export function shortUrl(url: string, max = 32): string {
  try {
    const u = new URL(url);
    const s = (u.host.replace(/^www\./, "") + u.pathname).replace(/\/$/, "");
    return s.length > max ? `${s.slice(0, max - 1)}…` : s;
  } catch {
    return url;
  }
}

/** The line to suggest for a link: "Articulated dragon (Printables)". */
export function linkTitle(p: Pick<LinkPreview, "title" | "site" | "url">): string {
  const title = p.title?.trim();
  if (!title) return "";
  const site = p.site?.trim();
  if (!site || title.toLowerCase().includes(site.toLowerCase())) return title;
  return `${title} (${site})`;
}

/** What a phone's share sheet sent, tidied into a line, a link and extra text. */
export function fromShare(s: { title?: string | null; text?: string | null; url?: string | null }): {
  text: string;
  url: string | null;
  body: string;
} {
  const url = s.url?.trim() || findUrl(s.text ?? "") || findUrl(s.title ?? "");
  let text = (s.title ?? "").trim();
  let body = (s.text ?? "").trim();
  if (url) {
    text = text.replace(url, "").trim();
    body = body.replace(url, "").trim();
  }
  if (!text && body && !body.includes("\n") && body.length <= 120) {
    text = body;
    body = "";
  }
  return { text, url: url || null, body };
}

/** "Fri 13 Nov" from YYYY-MM-DD. */
export function dueText(date: string, today: string): string {
  if (date === today) return "Today";
  const d = new Date(`${date}T12:00:00`);
  const t = new Date(`${today}T12:00:00`);
  const days = Math.round((d.getTime() - t.getTime()) / 86_400_000);
  if (days === 1) return "Tomorrow";
  if (days < 0) return days === -1 ? "Yesterday" : `${-days} days late`;
  return d.toLocaleDateString("en-GB", { weekday: "short", day: "numeric", month: "short" });
}

/** "Tuesday 10 November". */
export function longDate(date: string): string {
  return new Date(`${date}T12:00:00`).toLocaleDateString("en-GB", { weekday: "long", day: "numeric", month: "long" });
}

/** "Tonight" when every session starts at 5 pm or later, else "Today". */
export function sessionsHeading(starts: Date[]): string {
  return starts.length > 0 && starts.every((d) => d.getHours() >= 17) ? "Tonight" : "Today";
}

/** Where a search result opens. */
export function hitPath(h: SearchHit): string {
  switch (h.kind) {
    case "project":
      return `/projects/${h.id}`;
    case "task":
    case "comment":
      return `/projects/${h.project_id}/board?task=${h.task_id}`;
    case "note":
      return `/notes/${h.id}`;
    case "row":
      return `/projects/${h.project_id}/data/${h.database_id}?row=${h.id}`;
    default:
      return "/";
  }
}

/** Palette filters, in the order Tab steps through them. */
export const FILTERS = [
  { id: "all", label: "All", kinds: null },
  { id: "tasks", label: "Tasks", kinds: ["task", "comment"] },
  { id: "notes", label: "Notes", kinds: ["note"] },
  { id: "data", label: "Data", kinds: ["row"] },
  { id: "projects", label: "Projects", kinds: ["project"] },
] as const;
export type FilterId = (typeof FILTERS)[number]["id"];

/** Hits a filter keeps. */
export function filterHits(hits: SearchHit[], filter: FilterId): SearchHit[] {
  const f = FILTERS.find((x) => x.id === filter);
  if (!f?.kinds) return hits;
  return hits.filter((h) => (f.kinds as readonly string[]).includes(h.kind));
}

/** The palette's groups, in the mockup's order. */
export function groupHits(hits: SearchHit[]): { title: string; hits: SearchHit[] }[] {
  const groups = [
    { title: "Tasks", hits: hits.filter((h) => h.kind === "task" || h.kind === "comment") },
    { title: "Notes and data", hits: hits.filter((h) => h.kind === "note" || h.kind === "row") },
    { title: "Projects", hits: hits.filter((h) => h.kind === "project") },
  ];
  return groups.filter((g) => g.hits.length > 0);
}

/** Whether every word of the query appears in the text, ignoring case. */
export function matches(query: string, text: string): boolean {
  const t = text.toLowerCase();
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => t.includes(w));
}

/** Snippet text with its `[hit]` markers, as parts to render. */
export function snippetParts(s: string): { text: string; hit: boolean }[] {
  const out: { text: string; hit: boolean }[] = [];
  const re = /\[([^\]]*)\]/g;
  let last = 0;
  for (let m = re.exec(s); m; m = re.exec(s)) {
    if (m.index > last) out.push({ text: s.slice(last, m.index), hit: false });
    out.push({ text: m[1]!, hit: true });
    last = m.index + m[0].length;
  }
  if (last < s.length) out.push({ text: s.slice(last), hit: false });
  return out;
}

const RECENT_KEY = "tmp.palette.recent";

/** Recently opened palette results, newest first. */
export function readRecent(): SearchHit[] {
  try {
    const v = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
    return Array.isArray(v) ? (v as SearchHit[]) : [];
  } catch {
    return [];
  }
}

export function pushRecent(hit: SearchHit): void {
  try {
    const next = [hit, ...readRecent().filter((h) => !(h.kind === hit.kind && h.id === hit.id))].slice(0, 8);
    localStorage.setItem(RECENT_KEY, JSON.stringify(next));
  } catch {
    /* private mode: no recents */
  }
}

const LAST_PROJECT_KEY = "tmp.quickadd.project";

export function lastProject(): string {
  try {
    return localStorage.getItem(LAST_PROJECT_KEY) ?? "";
  } catch {
    return "";
  }
}

export function rememberProject(id: string): void {
  try {
    localStorage.setItem(LAST_PROJECT_KEY, id);
  } catch {
    /* nothing to remember with */
  }
}
