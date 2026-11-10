import type { components } from "../api/schema";

export type Database = components["schemas"]["Database"];
export type DatabaseDetail = components["schemas"]["DatabaseDetail"];
export type DatabaseRef = components["schemas"]["DatabaseRef"];
export type Column = components["schemas"]["Column"];
export type ColumnOptions = components["schemas"]["ColumnOptions"];
export type Choice = components["schemas"]["Choice"];
export type View = components["schemas"]["View"];
export type ViewConfig = components["schemas"]["ViewConfig"];
export type Filter = components["schemas"]["Filter"];
export type Row = components["schemas"]["Row"];
export type RelationRef = components["schemas"]["RelationRef"];
export type LinkedRows = components["schemas"]["LinkedRows"];

export type Kind =
  | "text"
  | "number"
  | "currency"
  | "select"
  | "multi_select"
  | "date"
  | "checkbox"
  | "url"
  | "person"
  | "relation";

export const KINDS: Kind[] = ["text", "number", "currency", "select", "multi_select", "date", "checkbox", "url", "person", "relation"];

export const KIND_LABEL: Record<Kind, string> = {
  text: "Text",
  number: "Number",
  currency: "Money",
  select: "Select",
  multi_select: "Multi-select",
  date: "Date",
  checkbox: "Checkbox",
  url: "Link",
  person: "Person",
  relation: "Relation",
};

/** A short glyph per type for column headers. */
export const KIND_ICON: Record<Kind, string> = {
  text: "Aa",
  number: "#",
  currency: "£",
  select: "▾",
  multi_select: "☰",
  date: "◷",
  checkbox: "☑",
  url: "↗",
  person: "@",
  relation: "⇄",
};

export const TARGET_LABEL: Record<string, string> = {
  task: "Tasks",
  note: "Notes",
  project: "Projects",
  row: "Rows of a database",
};

export const VIEW_LABEL: Record<string, string> = { table: "Table", board: "Board", calendar: "Calendar", gallery: "Gallery" };

export const OPS: { op: string; label: string; needsValue: boolean }[] = [
  { op: "is", label: "is", needsValue: true },
  { op: "is_not", label: "is not", needsValue: true },
  { op: "contains", label: "contains", needsValue: true },
  { op: "gt", label: "more than", needsValue: true },
  { op: "lt", label: "less than", needsValue: true },
  { op: "empty", label: "is empty", needsValue: false },
  { op: "not_empty", label: "is not empty", needsValue: false },
];

/** Which filter ops make sense for a column type. */
export function opsFor(kind: string): string[] {
  switch (kind) {
    case "number":
    case "currency":
    case "date":
      return ["is", "is_not", "gt", "lt", "empty", "not_empty"];
    case "checkbox":
      return ["is"];
    case "select":
    case "multi_select":
    case "person":
      return ["is", "is_not", "empty", "not_empty"];
    case "relation":
      return ["contains", "empty", "not_empty"];
    default:
      return ["contains", "is", "is_not", "empty", "not_empty"];
  }
}

const SYMBOL: Record<string, string> = { GBP: "£", USD: "$", EUR: "€", JPY: "¥", AUD: "A$", CAD: "C$" };

export function money(n: number, currency?: string | null): string {
  const sym = SYMBOL[currency ?? ""] ?? "";
  const s = n.toLocaleString("en-GB", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
  return sym ? `${n < 0 ? "-" : ""}${sym}${s.replace("-", "")}` : `${s}${currency ? ` ${currency}` : ""}`;
}

export function choiceName(column: Column, id: unknown): string {
  return column.options.choices.find((c) => c.id === id)?.name ?? "";
}

/** A cell as plain text. People need `names` (user id to name). */
export function display(column: Column, row: Row, names: Record<string, string> = {}): string {
  if (column.kind === "relation") return (row.relations[column.id] ?? []).map((r) => r.title).join(", ");
  const v = (row.values as Record<string, unknown>)[column.id];
  if (v === undefined || v === null) return "";
  switch (column.kind) {
    case "select":
      return choiceName(column, v);
    case "multi_select":
      return (v as string[]).map((id) => choiceName(column, id)).filter(Boolean).join(", ");
    case "checkbox":
      return v ? "Yes" : "No";
    case "currency":
      return money(Number(v), column.options.currency);
    case "number":
      return Number(v).toLocaleString("en-GB", { maximumFractionDigits: 6 });
    case "date":
      return shortDate(String(v));
    case "person":
      return names[String(v)] ?? "Someone";
    default:
      return String(v);
  }
}

export function shortDate(iso: string): string {
  const d = new Date(`${iso}T00:00:00`);
  if (Number.isNaN(d.getTime())) return iso;
  const sameYear = d.getFullYear() === new Date().getFullYear();
  return d.toLocaleDateString("en-GB", { day: "numeric", month: "short", ...(sameYear ? {} : { year: "numeric" }) });
}

export function cell(row: Row, columnId: string): unknown {
  return (row.values as Record<string, unknown>)[columnId];
}

/** The total of a number or money column across rows. */
export function total(column: Column, rows: Row[]): number {
  return rows.reduce((sum, r) => sum + (typeof cell(r, column.id) === "number" ? (cell(r, column.id) as number) : 0), 0);
}

/** Columns a view shows, the name column always first. */
export function visibleColumns(columns: Column[], view?: View | null): Column[] {
  const hidden = new Set(view?.config.hidden ?? []);
  const title = columns.filter((c) => c.is_title);
  return [...title, ...columns.filter((c) => !c.is_title && !hidden.has(c.id))];
}

export function rowTitle(columns: Column[], row: Row): string {
  const t = columns.find((c) => c.is_title);
  const v = t ? cell(row, t.id) : undefined;
  return typeof v === "string" && v.trim() ? v : "Untitled";
}

/**
 * An embed target from `![[Parts list#To order]]`: the database name and an
 * optional view name.
 */
export function parseEmbed(target: string): { database: string; view: string | null } {
  const [db = "", view] = target.split("#", 2);
  return { database: db.trim(), view: view?.trim() || null };
}

/** Find the database an embed names, preferring the note's own projects. */
export function resolveEmbed(
  target: string,
  databases: DatabaseRef[],
  projectIds: string[] = [],
): { database: DatabaseRef; view: View | null } | null {
  const { database, view } = parseEmbed(target);
  const named = databases.filter((d) => d.name.toLowerCase() === database.toLowerCase());
  const db = named.find((d) => projectIds.includes(d.project_id)) ?? named[0];
  if (!db) return null;
  const v = view ? (db.views.find((x) => x.name.toLowerCase() === view.toLowerCase()) ?? null) : null;
  return { database: db, view: v ?? db.views[0] ?? null };
}

/** The days of a month grid, Monday first, padded with the days around it. */
export function monthGrid(year: number, month: number): { iso: string; inMonth: boolean }[] {
  const first = new Date(Date.UTC(year, month, 1));
  const offset = (first.getUTCDay() + 6) % 7;
  const start = new Date(Date.UTC(year, month, 1 - offset));
  const days = [];
  for (let i = 0; i < 42; i++) {
    const d = new Date(start.getTime() + i * 86_400_000);
    days.push({ iso: d.toISOString().slice(0, 10), inMonth: d.getUTCMonth() === month });
  }
  // Drop a trailing week that is all next month.
  return days.slice(35).every((d) => !d.inMonth) ? days.slice(0, 35) : days;
}

export const SLOTS = ["slot-1", "slot-2", "slot-3", "slot-4", "slot-5", "slot-6", "slot-7", "slot-8"];
