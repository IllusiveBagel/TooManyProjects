import type { Project } from "../api/client";

/** "Today", "Yesterday", "3d ago", "2w ago", "4mo ago", "1y ago". */
export function lastActive(iso: string, now: Date = new Date()): string {
  const then = new Date(iso);
  const startOf = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((startOf(now) - startOf(then)) / 86_400_000);
  if (days <= 0) return "Today";
  if (days === 1) return "Yesterday";
  if (days < 14) return `${days}d ago`;
  if (days < 60) return `${Math.floor(days / 7)}w ago`;
  if (days < 365) return `${Math.floor(days / 30)}mo ago`;
  return `${Math.floor(days / 365)}y ago`;
}

/** Due date for a card footer: "Due today", "Due Fri" within a week, "Due 3 Mar", or "Overdue". */
export function dueLabel(date: string, now: Date = new Date()): string {
  const [y, m, d] = date.split("-").map(Number) as [number, number, number];
  const due = new Date(y, m - 1, d);
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const days = Math.round((due.getTime() - today.getTime()) / 86_400_000);
  if (days < 0) return "Overdue";
  if (days === 0) return "Due today";
  if (days < 7) return `Due ${due.toLocaleDateString("en-GB", { weekday: "short" })}`;
  const sameYear = due.getFullYear() === today.getFullYear();
  return `Due ${due.toLocaleDateString("en-GB", { day: "numeric", month: "short", ...(sameYear ? {} : { year: "2-digit" }) })}`;
}

/** Share of tasks done, 0 to 100. */
export function progress(project: Pick<Project, "stats">): number {
  const { total_tasks, done_tasks } = project.stats;
  return total_tasks === 0 ? 0 : Math.round((done_tasks / total_tasks) * 100);
}

export const STATUS_LABEL: Record<string, string> = {
  idea: "Idea",
  active: "Active",
  paused: "Paused",
  done: "Done",
  archived: "Archived",
};

/** Text colour class for a status tag. */
export const STATUS_TONE: Record<string, string> = {
  idea: "text-text-quiet",
  active: "text-accent",
  paused: "text-slot-6",
  done: "text-done",
  archived: "text-text-quiet",
};

export const STATUS_ORDER = ["idea", "active", "paused", "done", "archived"] as const;
