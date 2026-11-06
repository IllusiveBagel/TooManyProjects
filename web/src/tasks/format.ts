import type { ActivityItem, Status, Task } from "../api/client";

export const CATEGORIES = ["todo", "in_progress", "blocked", "done"] as const;
export type Category = (typeof CATEGORIES)[number];

export const CATEGORY_LABEL: Record<string, string> = {
  todo: "To do",
  in_progress: "In progress",
  blocked: "Blocked",
  done: "Done",
};

/** CSS colour for a column category, from theme tokens only. */
export const CATEGORY_COLOR: Record<string, string> = {
  todo: "var(--tmp-text-quiet)",
  in_progress: "var(--tmp-slot-6)",
  blocked: "var(--tmp-late)",
  done: "var(--tmp-done)",
};

export const PRIORITIES = ["low", "medium", "high", "urgent"] as const;
export const PRIORITY_LABEL: Record<string, string> = { low: "Low", medium: "Med", high: "High", urgent: "Urgent" };

/** "T-12". */
export const taskRef = (t: Pick<Task, "number">) => `T-${t.number}`;

/** Task ids per column, in board order. Tasks for unknown columns are dropped. */
export function groupByColumn(tasks: Task[], columnIds: string[], key: (t: Task) => string): Record<string, string[]> {
  const out: Record<string, string[]> = Object.fromEntries(columnIds.map((id) => [id, []]));
  for (const t of tasks) out[key(t)]?.push(t.id);
  return out;
}

/** Which column holds `id`: a column id itself, or the column of a card. */
export function findColumn(order: Record<string, string[]>, id: string): string | undefined {
  if (id in order) return id;
  return Object.keys(order).find((col) => order[col]!.includes(id));
}

/**
 * Moves a card into `toColumn` at `index` (clamped), returning a new order.
 * The card is taken out of wherever it was first.
 */
export function moveCard(
  order: Record<string, string[]>,
  cardId: string,
  toColumn: string,
  index: number,
): Record<string, string[]> {
  const next: Record<string, string[]> = {};
  for (const [col, ids] of Object.entries(order)) next[col] = ids.filter((id) => id !== cardId);
  const target = next[toColumn] ?? [];
  const at = Math.max(0, Math.min(index, target.length));
  next[toColumn] = [...target.slice(0, at), cardId, ...target.slice(at)];
  return next;
}

/** The card a dropped card now sits under, or null when it is at the top. */
export function cardAbove(order: Record<string, string[]>, column: string, cardId: string): string | null {
  const ids = order[column] ?? [];
  const i = ids.indexOf(cardId);
  return i > 0 ? ids[i - 1]! : null;
}

/**
 * Applies a move to a cached task list for instant feedback: the task takes
 * the new column and sits right after `afterId` (or first in the column).
 */
export function applyMove(
  tasks: Task[],
  taskId: string,
  status: Pick<Status, "id" | "category">,
  afterId: string | null,
): Task[] {
  const moving = tasks.find((t) => t.id === taskId);
  if (!moving) return tasks;
  const moved: Task = {
    ...moving,
    status_id: status.id,
    category: status.category,
    overdue: status.category === "done" ? false : moving.overdue,
  };
  const rest = tasks.filter((t) => t.id !== taskId);
  let at: number;
  if (afterId) {
    at = rest.findIndex((t) => t.id === afterId) + 1;
  } else {
    const first = rest.findIndex((t) => t.status_id === status.id);
    // Empty column: put it after the last card of any earlier column, which
    // keeps the list grouped. Grouping by column makes the exact spot moot.
    at = first === -1 ? rest.length : first;
  }
  return [...rest.slice(0, at), moved, ...rest.slice(at)];
}

const quote = (p: Record<string, unknown>) => (typeof p.title === "string" ? `“${p.title}”` : "a task");

/** One line for the activity feed. */
export function activityText(a: Pick<ActivityItem, "kind" | "payload">): string {
  const p = (a.payload ?? {}) as Record<string, unknown>;
  switch (a.kind) {
    case "task.created":
      return `added ${quote(p)}`;
    case "task.moved":
      return `moved ${quote(p)} from ${p.from} to ${p.to}`;
    case "task.completed":
      return `finished ${quote(p)}`;
    case "task.updated": {
      const fields = Array.isArray(p.fields) ? p.fields.join(", ") : "details";
      return `changed the ${fields} of ${quote(p)}`;
    }
    case "task.deleted":
      return `deleted ${quote(p)}`;
    case "status.deleted":
      return `removed the ${p.name} column`;
    case "project.created":
      return "started the project";
    case "project.archived":
      return "archived the project";
    case "project.restored":
      return "brought the project back";
    case "project.status_changed":
      return `marked the project ${p.to}`;
    case "project.updated":
      return "edited the project";
    case "link.created":
      return `drew a ${p.kind} link`;
    case "link.deleted":
      return `removed a ${p.kind} link`;
    default:
      return a.kind.replace(".", " ");
  }
}
