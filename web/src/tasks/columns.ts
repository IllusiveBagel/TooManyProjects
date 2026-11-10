import type { Status } from "../api/client";
import { CATEGORY_LABEL } from "./format";

/** Column names that read as "finished". The server's MCP briefing uses the same list. */
const DONE_NAMES = ["done", "finished", "complete", "completed", "published"];

/** Whether a column's name says its tasks are finished. */
export const soundsDone = (name: string) => DONE_NAMES.includes(name.trim().toLowerCase());

/** A column named like Done that doesn't count as done. */
export const misnamed = (s: Pick<Status, "name" | "category">) => s.category !== "done" && soundsDone(s.name);

export interface DoneWarning {
  /** `none`: no column counts as done. `misnamed`: one called Done doesn't. */
  kind: "none" | "misnamed";
  /** The column a one-tap fix would switch to Done, if there is an obvious one. */
  fix?: Status;
  title: string;
  detail: string;
}

/** Why a board might never count anything as finished, if it might. */
export function doneWarning(statuses: Status[]): DoneWarning | null {
  if (statuses.length === 0) return null;
  const wrong = statuses.find(misnamed);
  const counted = wrong ? `The “${wrong.name}” column counts as ${CATEGORY_LABEL[wrong.category]?.toLowerCase()}` : "";
  if (!statuses.some((s) => s.category === "done")) {
    return {
      kind: "none",
      fix: wrong,
      title: "Nothing on this board counts as finished.",
      detail: wrong
        ? `${counted}, so tasks there stay open on the canvas card, the global board and the dashboard.`
        : "No column counts as Done, so tasks never show as finished on the canvas card, the global board or the dashboard.",
    };
  }
  if (wrong) {
    return {
      kind: "misnamed",
      fix: wrong,
      title: `“${wrong.name}” doesn't count as done.`,
      detail: `${counted}, so tasks there stay open.`,
    };
  }
  return null;
}

// "Not now" on the warning lasts until the board's columns change. It is a
// per-browser convenience, so storage failing just shows the warning again.
const DISMISS_KEY = "tmp.doneWarning.dismissed";

export const boardSignature = (statuses: Status[]) => statuses.map((s) => `${s.id}:${s.name}:${s.category}`).join("|");

function readDismissed(): Record<string, string> {
  try {
    return JSON.parse(localStorage.getItem(DISMISS_KEY) ?? "{}") as Record<string, string>;
  } catch {
    return {};
  }
}

export function isWarningDismissed(projectId: string, statuses: Status[]) {
  return readDismissed()[projectId] === boardSignature(statuses);
}

export function dismissWarning(projectId: string, statuses: Status[]) {
  try {
    localStorage.setItem(DISMISS_KEY, JSON.stringify({ ...readDismissed(), [projectId]: boardSignature(statuses) }));
  } catch {
    // Private browsing: the warning just comes back next time.
  }
}

/**
 * The `after_id` that moves a column one place left (-1) or right (+1):
 * `null` means first, `undefined` means it can't move that way.
 */
export function afterForStep(statuses: Pick<Status, "id">[], id: string, step: -1 | 1): string | null | undefined {
  const i = statuses.findIndex((s) => s.id === id);
  const to = i + step;
  if (i === -1 || to < 0 || to >= statuses.length) return undefined;
  if (step === 1) return statuses[to]!.id;
  return to === 0 ? null : statuses[to - 1]!.id;
}

/** The list with one column moved after `afterId` (first when null), for showing a move before the server answers. */
export function moveColumn<T extends { id: string }>(list: T[], id: string, afterId: string | null): T[] {
  const moving = list.find((s) => s.id === id);
  if (!moving) return list;
  const rest = list.filter((s) => s.id !== id);
  const at = afterId === null ? 0 : rest.findIndex((s) => s.id === afterId) + 1;
  return [...rest.slice(0, at), moving, ...rest.slice(at)];
}

/** "after Doing", or "to the start". */
export function placeText(statuses: Pick<Status, "id" | "name">[], afterId: string | null) {
  if (afterId === null) return "to the start";
  return `after ${statuses.find((s) => s.id === afterId)?.name ?? "the column"}`;
}
