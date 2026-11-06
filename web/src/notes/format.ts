import type { components } from "../api/schema";

export type NoteSummary = components["schemas"]["NoteSummary"];
export type NoteDetail = components["schemas"]["NoteDetail"];
export type NoteLink = components["schemas"]["NoteLink"];

/** One `[[...]]` in a note body. */
export interface WikiTarget {
  project: boolean;
  title: string;
  heading?: string;
  /** Text to show: the alias after `|`, else the title. */
  label: string;
}

/** Same rules as the server: `[[Title]]`, `[[Title|alias]]`, `[[Title#heading]]`, `[[@Project]]`. */
export function parseWiki(inner: string): WikiTarget | null {
  const [targetPart, alias] = splitOnce(inner, "|");
  const [rawTarget, heading] = splitOnce(targetPart, "#");
  let target = rawTarget.trim();
  const project = target.startsWith("@");
  if (project) target = target.slice(1).trim();
  else {
    if (/\.[A-Za-z0-9]{1,5}$/.test(target) && !/\.md$/i.test(target)) return null; // an attachment
    target = target.replace(/\.md$/i, "");
    target = target.split("/").pop() ?? target;
  }
  if (!target) return null;
  const shown = alias?.trim() || (project ? `@${target}` : target);
  return { project, title: target, heading: heading?.trim() || undefined, label: shown };
}

function splitOnce(s: string, sep: string): [string, string | undefined] {
  const i = s.indexOf(sep);
  return i < 0 ? [s, undefined] : [s.slice(0, i), s.slice(i + 1)];
}

/** `wiki:` and `project:` hrefs carry the target through markdown rendering. */
export function wikiHref(t: WikiTarget): string {
  return `${t.project ? "project" : "wiki"}:${encodeURIComponent(t.title)}`;
}

export function readWikiHref(href: string): { project: boolean; title: string } | null {
  const m = /^(wiki|project):(.*)$/.exec(href);
  if (!m) return null;
  return { project: m[1] === "project", title: decodeURIComponent(m[2] ?? "") };
}

const escapeLabel = (s: string) => s.replace(/([\\[\]])/g, "\\$1");

/**
 * Turns wiki links into ordinary markdown links (`[label](wiki:Title)`) so a
 * markdown renderer can draw them. Fenced code and code spans are left alone,
 * as are `![[embeds]]`, which become plain text for now.
 */
export function wikiToMarkdown(body: string): string {
  let inFence = false;
  return body
    .split("\n")
    .map((line) => {
      const trimmed = line.trimStart();
      if (trimmed.startsWith("```") || trimmed.startsWith("~~~")) {
        inFence = !inFence;
        return line;
      }
      if (inFence) return line;
      let out = "";
      let inCode = false;
      for (let i = 0; i < line.length; ) {
        if (line[i] === "`") {
          inCode = !inCode;
          out += "`";
          i += 1;
          continue;
        }
        if (!inCode && line.startsWith("[[", i)) {
          const end = line.indexOf("]]", i + 2);
          if (end > 0) {
            const embed = out.endsWith("!");
            const target = parseWiki(line.slice(i + 2, end));
            if (embed) out = out.slice(0, -1);
            out += target && !embed ? `[${escapeLabel(target.label)}](${wikiHref(target)})` : line.slice(i + 2, end);
            i = end + 2;
            continue;
          }
        }
        out += line[i];
        i += 1;
      }
      return out;
    })
    .join("\n");
}

/** The open `[[` before the cursor, if the cursor is inside one: what's typed so far. */
export function openWikiQuery(before: string): { from: number; query: string; project: boolean } | null {
  const start = before.lastIndexOf("[[");
  if (start < 0) return null;
  const typed = before.slice(start + 2);
  if (typed.includes("]]") || typed.includes("\n")) return null;
  const project = typed.startsWith("@");
  return { from: start + 2 + (project ? 1 : 0), query: project ? typed.slice(1) : typed, project };
}

export interface FolderNode {
  name: string;
  path: string;
  count: number;
  children: FolderNode[];
}

/** Folders of the given note paths as a tree, with note counts (own and below). */
export function folderTree(paths: string[]): FolderNode[] {
  const root: FolderNode = { name: "", path: "", count: 0, children: [] };
  for (const p of paths) {
    const parts = p.split("/").slice(0, -1);
    let node = root;
    for (const part of parts) {
      const path = node.path ? `${node.path}/${part}` : part;
      let child = node.children.find((c) => c.name === part);
      if (!child) {
        child = { name: part, path, count: 0, children: [] };
        node.children.push(child);
      }
      child.count += 1;
      node = child;
    }
  }
  const order = ["Inbox", "Knowledge", "Projects"];
  const sort = (nodes: FolderNode[], top: boolean) => {
    nodes.sort((a, b) => {
      if (top) {
        const ia = order.indexOf(a.name);
        const ib = order.indexOf(b.name);
        if (ia !== ib) return (ia < 0 ? 99 : ia) - (ib < 0 ? 99 : ib);
      }
      return a.name.localeCompare(b.name);
    });
    nodes.forEach((n) => sort(n.children, false));
  };
  sort(root.children, true);
  return root.children;
}

/** Search snippets mark hits with `[` and `]`; split them for highlighting. */
export function snippetParts(snippet: string): { text: string; hit: boolean }[] {
  const out: { text: string; hit: boolean }[] = [];
  const re = /\[([^\]]*)\]/g;
  let last = 0;
  for (let m = re.exec(snippet); m; m = re.exec(snippet)) {
    if (m.index > last) out.push({ text: snippet.slice(last, m.index), hit: false });
    out.push({ text: m[1] ?? "", hit: true });
    last = m.index + m[0].length;
  }
  if (last < snippet.length) out.push({ text: snippet.slice(last), hit: false });
  return out;
}

/** "just now", "5 min ago", "3 h ago", "2 days ago", or the date. */
export function ago(iso: string, now = Date.now()): string {
  const s = Math.max(0, (now - new Date(iso).getTime()) / 1000);
  if (s < 45) return "just now";
  if (s < 3600) return `${Math.round(s / 60)} min ago`;
  if (s < 86400) return `${Math.round(s / 3600)} h ago`;
  if (s < 86400 * 7) {
    const d = Math.round(s / 86400);
    return d === 1 ? "yesterday" : `${d} days ago`;
  }
  return new Date(iso).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
}

/** Tags typed as `car, how to #brakes` → `["car", "how-to", "brakes"]`. */
export function parseTags(input: string): string[] {
  const out: string[] = [];
  for (const raw of input.split(/[,#]/)) {
    const t = raw.trim().replace(/\s+/g, "-");
    if (t && !out.some((o) => o.toLowerCase() === t.toLowerCase())) out.push(t);
  }
  return out;
}
