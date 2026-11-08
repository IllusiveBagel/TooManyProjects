import { useEffect, useState, type ReactNode } from "react";
import { Link, useSearchParams } from "react-router";
import type { TaskDetail } from "../api/client";
import { lastActive } from "../canvas/format";
import { useProjectDetail } from "../canvas/queries";
import { TaskLinkedRows } from "../data/LinkedRows";
import { useTaskRows } from "../data/queries";
import { useMembers } from "../sharing/queries";
import { Button, ErrorNote, ViaBadge } from "../components/ui";
import { projectColor } from "../theme/resolve";
import { activityText, CATEGORY_COLOR, PRIORITIES, PRIORITY_LABEL, taskRef } from "./format";
import {
  useChecklist,
  useCreateLabel,
  useDeleteTask,
  useLabels,
  useStatuses,
  useTaskDetail,
  useUpdateTask,
} from "./queries";
import { TaskLog, TaskNotes } from "./TaskLog";

const inputClass = "border-rule bg-surface text-text w-full rounded-md px-3 py-2.5 outline-none focus:border-accent";

/** The open task lives in the `?task=` search param, so it survives reloads and can be linked. */
export function useOpenTask() {
  const [params, setParams] = useSearchParams();
  const id = params.get("task");
  const open = (taskId: string | null) =>
    setParams(
      (p) => {
        const next = new URLSearchParams(p);
        if (taskId) next.set("task", taskId);
        else next.delete("task");
        return next;
      },
      { replace: !!id && !!taskId },
    );
  return [id, open] as const;
}

/** Side panel for one task. Fields save as they change or lose focus. */
export function TaskDrawer() {
  const [id, open] = useOpenTask();
  const detail = useTaskDetail(id);

  useEffect(() => {
    if (!id) return;
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
      if (e.key === "Escape" && !typing) open(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  if (!id) return null;
  return (
    <>
      <div className="bg-shadow/40 fixed inset-0 z-30 sm:hidden" onClick={() => open(null)} aria-hidden="true" />
      <aside
        aria-label="Task"
        className="border-rule bg-surface-card fixed inset-x-0 bottom-0 z-40 flex max-h-[88vh] flex-col overflow-hidden rounded-t-lg border-b-0 sm:absolute sm:inset-y-0 sm:right-0 sm:left-auto sm:max-h-none sm:w-[440px] sm:rounded-none sm:border-y-0 sm:border-r-0"
      >
        {detail.data ? (
          <Body key={detail.data.task.id} detail={detail.data} onClose={() => open(null)} />
        ) : detail.isError ? (
          <div className="flex flex-col gap-3 p-5">
            <ErrorNote error={detail.error} />
            <Button onClick={() => open(null)}>Close</Button>
          </div>
        ) : (
          <p className="label-mono text-text-quiet p-5">Loading…</p>
        )}
      </aside>
    </>
  );
}

function Body({ detail, onClose }: { detail: TaskDetail; onClose: () => void }) {
  const { task, checklist } = detail;
  // Comments have their own log, so the history leaves them out.
  const activity = detail.activity.filter((a) => a.kind !== "task.commented");
  const statuses = useStatuses(task.project_id).data ?? [];
  const labels = useLabels(task.project_id).data ?? [];
  const update = useUpdateTask();
  const remove = useDeleteTask();
  const createLabel = useCreateLabel(task.project_id);
  const items = useChecklist(task.id);
  const role = useProjectDetail(task.project_id).data?.project.role;
  const canEdit = role !== "viewer";
  const linkedRows = useTaskRows(task.id);
  const members = useMembers(task.project_id).data ?? [];
  const [title, setTitle] = useState(task.title);
  const [description, setDescription] = useState(task.description ?? "");
  const [newItem, setNewItem] = useState("");
  const [newLabel, setNewLabel] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);
  const save = (fields: Omit<Parameters<typeof update.mutate>[0], "id">) => update.mutate({ id: task.id, ...fields });
  const labelIds = task.labels.map((l) => l.id);
  const toggleLabel = (id: string) =>
    save({ label_ids: labelIds.includes(id) ? labelIds.filter((l) => l !== id) : [...labelIds, id] });

  return (
    <>
      <div className="flex items-start gap-3 border-t-[6px] px-5 pt-4" style={{ borderTopColor: projectColor(task.project_color) }}>
        <div className="flex flex-1 flex-col gap-1">
          <span className="label-mono text-text-quiet text-[11px]">
            <Link to={`/projects/${task.project_id}`} className="hover:text-text underline-offset-2 hover:underline">
              {task.project_title}
            </Link>{" "}
            · {taskRef(task)}
            {detail.comments.length > 0 && (
              <>
                {" · "}
                <JumpTo id="task-log">Log {detail.comments.length}</JumpTo>
              </>
            )}
            {detail.notes.length + detail.hidden_notes > 0 && (
              <>
                {" · "}
                <JumpTo id="task-notes">
                  {detail.notes.length + detail.hidden_notes} note{detail.notes.length + detail.hidden_notes === 1 ? "" : "s"}
                </JumpTo>
              </>
            )}
          </span>
          <label>
            <span className="sr-only">Title</span>
            <textarea
              rows={1}
              className="font-display text-text field-sizing-content w-full resize-none bg-transparent text-[24px] leading-tight font-extrabold tracking-[-0.02em] outline-none"
              value={title}
              maxLength={300}
              readOnly={!canEdit}
              onChange={(e) => setTitle(e.target.value.replace(/\n/g, " "))}
              onKeyDown={(e) => e.key === "Enter" && (e.preventDefault(), e.currentTarget.blur())}
              onBlur={() => title.trim() && title !== task.title && save({ title })}
            />
          </label>
        </div>
        <button
          type="button"
          onClick={onClose}
          aria-label="Close"
          className="text-text-quiet hover:text-text grid size-11 shrink-0 cursor-pointer place-items-center text-2xl"
        >
          ×
        </button>
      </div>

      <div className="flex flex-1 flex-col gap-5 overflow-y-auto px-5 pt-3 pb-6">
        {!canEdit && <p className="label-mono text-text-quiet text-[11px]">View only: this project is shared with you to look at.</p>}
        {/* A viewer sees every field, read-only. */}
        <fieldset disabled={!canEdit} className="flex min-w-0 flex-col gap-5">
          <div className="grid grid-cols-2 gap-3">
            <label className="flex flex-col gap-1.5">
              <span className="label-mono text-text-quiet">Column</span>
              <select
                className={`${inputClass} font-mono text-[13px]`}
                value={task.status_id}
                onChange={(e) => save({ status_id: e.target.value })}
                style={{ borderLeftWidth: 6, borderLeftColor: CATEGORY_COLOR[task.category] }}
              >
                {statuses.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.name}
                  </option>
                ))}
              </select>
            </label>
            <label className="flex flex-col gap-1.5">
              <span className="label-mono text-text-quiet">Due</span>
              <input
                type="date"
                className={`${inputClass} font-mono text-[13px] ${task.overdue ? "text-late" : ""}`}
                value={task.due_date ?? ""}
                onChange={(e) => save({ due_date: e.target.value || null })}
              />
            </label>
          </div>

          {members.length > 1 && (
            <label className="flex flex-col gap-1.5">
              <span className="label-mono text-text-quiet">Assigned to</span>
              <select
                className={`${inputClass} font-mono text-[13px]`}
                value={task.assignee_id ?? ""}
                onChange={(e) => save({ assignee_id: e.target.value || null })}
              >
                <option value="">Nobody</option>
                {members.map((m) => (
                  <option key={m.user_id} value={m.user_id}>
                    {m.name}
                  </option>
                ))}
              </select>
            </label>
          )}

          <fieldset className="flex flex-col gap-1.5">
            <legend className="label-mono text-text-quiet mb-1.5">Priority</legend>
            <div className="border-rule inline-flex w-fit overflow-hidden rounded-md">
              {[null, ...PRIORITIES].map((p) => {
                const on = task.priority === p;
                return (
                  <button
                    key={p ?? "none"}
                    type="button"
                    aria-pressed={on}
                    onClick={() => save({ priority: p })}
                    className={`label-mono min-h-11 cursor-pointer px-3 ${on ? "bg-accent text-on-accent" : "text-text-quiet hover:text-text"}`}
                  >
                    {p ? PRIORITY_LABEL[p] : "None"}
                  </button>
                );
              })}
            </div>
          </fieldset>

          <fieldset className="flex flex-col gap-1.5">
            <legend className="label-mono text-text-quiet mb-1.5">Labels</legend>
            <div className="flex flex-wrap gap-1.5">
              {labels.map((l) => {
                const on = labelIds.includes(l.id);
                return (
                  <button
                    key={l.id}
                    type="button"
                    aria-pressed={on}
                    onClick={() => toggleLabel(l.id)}
                    className={`border-rule label-mono min-h-9 cursor-pointer rounded-sm px-2 text-[11px] ${on ? "border-text text-text" : "text-text-quiet border-dashed"}`}
                    style={on ? { background: `color-mix(in srgb, ${l.color ? projectColor(l.color) : "var(--tmp-text-quiet)"} 22%, transparent)` } : undefined}
                  >
                    {on ? "✓ " : ""}
                    {l.name}
                  </button>
                );
              })}
              {canEdit && (
                <form
                  className="flex"
                  onSubmit={(e) => {
                    e.preventDefault();
                    const name = newLabel.trim();
                    if (!name) return;
                    const slot = `slot-${(labels.length % 8) + 1}`;
                    createLabel.mutate(
                      { name, color: slot },
                      { onSuccess: (l) => (setNewLabel(""), save({ label_ids: [...labelIds, l.id] })) },
                    );
                  }}
                >
                  <input
                    className="border-rule bg-surface label-mono min-h-9 w-28 rounded-sm px-2 text-[11px] outline-none focus:border-accent"
                    placeholder="+ New label"
                    value={newLabel}
                    maxLength={40}
                    onChange={(e) => setNewLabel(e.target.value)}
                    aria-label="New label"
                  />
                </form>
              )}
            </div>
          </fieldset>

          <label className="flex flex-col gap-1.5">
            <span className="label-mono text-text-quiet">Notes · markdown</span>
            <textarea
              className={`${inputClass} font-reading min-h-32 text-[15px] leading-relaxed`}
              value={description}
              placeholder={canEdit ? "What needs doing, links, measurements…" : "No notes."}
              onChange={(e) => setDescription(e.target.value)}
              onBlur={() => description !== (task.description ?? "") && save({ description: description || null })}
            />
          </label>

          <section className="flex flex-col gap-1.5">
            <h3 className="label-mono text-text-quiet">
              Checklist {checklist.length > 0 && `· ${checklist.filter((c) => c.done).length}/${checklist.length}`}
            </h3>
            <ul className="flex flex-col">
              {checklist.map((c) => (
                <li key={c.id} className="group flex min-h-10 items-center gap-2.5">
                  <input
                    type="checkbox"
                    checked={c.done}
                    onChange={(e) => items.update.mutate({ id: c.id, done: e.target.checked })}
                    className="accent-accent size-4.5 cursor-pointer"
                    aria-label={c.text}
                  />
                  <span className={`flex-1 ${c.done ? "text-text-quiet line-through" : ""}`}>{c.text}</span>
                  <button
                    type="button"
                    onClick={() => items.remove.mutate(c.id)}
                    aria-label={`Remove ${c.text}`}
                    className={`text-text-quiet hover:text-late size-9 cursor-pointer opacity-0 group-hover:opacity-100 focus:opacity-100 ${canEdit ? "" : "hidden"}`}
                  >
                    ×
                  </button>
                </li>
              ))}
            </ul>
            {canEdit && (
              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  if (newItem.trim()) items.add.mutate(newItem.trim(), { onSuccess: () => setNewItem("") });
                }}
              >
                <input
                  className={`${inputClass} text-sm`}
                  placeholder="+ Add an item"
                  value={newItem}
                  maxLength={300}
                  onChange={(e) => setNewItem(e.target.value)}
                  aria-label="New checklist item"
                />
              </form>
            )}
          </section>
        </fieldset>
        <TaskLinkedRows groups={linkedRows.data ?? []} taskId={task.id} canEdit={canEdit} />
        <TaskNotes detail={detail} canEdit={canEdit} />
        <TaskLog detail={detail} canEdit={canEdit} />

        <ErrorNote error={update.error ?? remove.error ?? createLabel.error ?? items.add.error} />

        {activity.length > 0 && (
          <section className="flex flex-col gap-2">
            <h3 className="label-mono text-text-quiet">History</h3>
            <ol className="flex flex-col gap-1.5 text-sm">
              {activity.map((a) => (
                <li key={a.id} className="flex gap-2">
                  <span className="label-mono text-text-quiet w-20 shrink-0 text-[10px] leading-5">
                    {lastActive(a.created_at)}
                  </span>
                  <span>
                    <b>{a.user_name ?? "Someone"}</b> {activityText(a)}
                    <ViaBadge via={a.via} />
                  </span>
                </li>
              ))}
            </ol>
          </section>
        )}

        {canEdit && (
          <div className="border-rule flex flex-wrap gap-2 border-x-0 border-b-0 pt-4">
            {confirmDelete ? (
              <>
                <Button
                  type="button"
                  className="text-late border-current"
                  onClick={() => remove.mutate(task.id, { onSuccess: onClose })}
                >
                  Delete for good
                </Button>
                <Button type="button" onClick={() => setConfirmDelete(false)}>Keep it</Button>
              </>
            ) : (
              <Button type="button" className="text-late" onClick={() => setConfirmDelete(true)}>
                Delete task…
              </Button>
            )}
          </div>
        )}
      </div>
    </>
  );
}

/** A small link in the drawer's header that scrolls to a section below. */
function JumpTo({ id, children }: { id: string; children: ReactNode }) {
  return (
    <button
      type="button"
      className="text-accent cursor-pointer underline-offset-2 hover:underline"
      onClick={() => document.getElementById(id)?.scrollIntoView({ behavior: "smooth", block: "start" })}
    >
      {children}
    </button>
  );
}
