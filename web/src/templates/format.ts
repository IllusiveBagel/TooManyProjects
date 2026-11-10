import type { ProjectTemplate } from "../api/client";

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

/** "a, b and c". */
export function listJoin(items: string[]): string {
  if (items.length <= 1) return items.join("");
  return `${items.slice(0, -1).join(", ")} and ${items[items.length - 1]}`;
}

/** What a template brings, for the line under the picker:
 *  "6 columns, a Parts database and 3 labels". */
export function bringsText(t: ProjectTemplate): string {
  const parts = [plural(t.columns.length, "column")];
  if (t.database_names.length === 1)
    parts.push(`a ${t.database_names[0]} database`);
  else if (t.database_names.length > 1)
    parts.push(plural(t.database_names.length, "database"));
  if (t.label_names.length) parts.push(plural(t.label_names.length, "label"));
  if (t.task_count) parts.push(plural(t.task_count, "starter task"));
  if (t.note_titles.length) parts.push(plural(t.note_titles.length, "note"));
  return listJoin(parts);
}

/** The phone picker's one line: the column names for a built-in, counts for one you saved. */
export function compactLine(t: ProjectTemplate): string {
  if (t.builtin) return t.columns.map((c) => c.name).join(" · ");
  const parts = [plural(t.columns.length, "column")];
  if (t.task_count) parts.push(plural(t.task_count, "starter task"));
  return parts.join(" · ");
}

export const BLANK_ID = "builtin-blank";
