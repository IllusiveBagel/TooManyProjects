import { useState } from "react";
import { Link } from "react-router";
import type { Task } from "../api/client";
import { useCanvas } from "../canvas/queries";
import { ErrorNote } from "../components/ui";
import { Popover } from "../data/Cell";
import { CATEGORY_COLOR, CATEGORY_LABEL, taskRef } from "../tasks/format";
import { useGlobalTasks, useTaskNoteLinks } from "../tasks/queries";
import { projectColor } from "../theme/resolve";
import type { NoteDetail } from "./format";

/** In a note's side panel: the tasks it is linked to, and a way to link another. */
export function NoteTasks({ detail }: { detail: NoteDetail }) {
  const { note, tasks } = detail;
  const [picking, setPicking] = useState(false);
  const links = useTaskNoteLinks();
  const projects = useCanvas(false).data?.projects ?? [];
  const editable = new Set(projects.filter((p) => p.role !== "viewer").map((p) => p.id));
  const canLink = editable.size > 0;

  return (
    <section className="flex flex-col gap-2" aria-label="Linked tasks">
      <h2 className="label-mono text-text-quiet text-[11px] font-normal">
        Tasks <span className="text-accent">{tasks.length}</span>
      </h2>
      {tasks.length === 0 && (
        <p className="text-text-quiet text-[13px]">Not linked to a task. Link one to see this note when you open it.</p>
      )}
      <ul className="flex flex-col gap-1.5">
        {tasks.map((t) => (
          <li key={t.id} className="group flex items-stretch">
            <TaskRow task={t} />
            {editable.has(t.project_id) && (
              <button
                type="button"
                onClick={() => links.unlink.mutate({ taskId: t.id, noteId: note.id })}
                aria-label={`Unlink ${taskRef(t)} ${t.title}`}
                title="Unlink"
                className="text-text-quiet hover:text-late w-9 shrink-0 cursor-pointer lg:opacity-0 lg:group-hover:opacity-100 lg:focus:opacity-100"
              >
                ×
              </button>
            )}
          </li>
        ))}
      </ul>
      {canLink && (
        <div className="relative">
          <button
            type="button"
            aria-expanded={picking}
            onClick={() => setPicking(true)}
            className="label-mono text-text-quiet hover:text-text border-rule min-h-9 w-full cursor-pointer rounded-md border-dashed px-2.5 text-left text-[11px]"
          >
            + Link a task
          </button>
          {picking && (
            <TaskPicker
              linked={tasks.map((t) => t.id)}
              editable={editable}
              onPick={(taskId) => links.link.mutate({ taskId, noteId: note.id }, { onSuccess: () => setPicking(false) })}
              onClose={() => setPicking(false)}
            />
          )}
        </div>
      )}
      <ErrorNote error={links.link.error ?? links.unlink.error} />
    </section>
  );
}

function TaskRow({ task: t }: { task: Task }) {
  const done = t.category === "done";
  return (
    <Link
      to={`/projects/${t.project_id}?task=${t.id}`}
      className="border-rule bg-surface-card hover:border-accent flex min-w-0 flex-1 flex-col gap-1 rounded-md px-3 py-2"
      style={{ borderLeftWidth: 5, borderLeftColor: projectColor(t.project_color) }}
    >
      <span className={`text-[14px] leading-snug font-bold ${done ? "text-text-quiet line-through decoration-1" : ""}`}>{t.title}</span>
      <span className="label-mono text-text-quiet flex flex-wrap gap-x-2 text-[10px]">
        <span>{taskRef(t)}</span>
        <span className="truncate">{t.project_title}</span>
        <span style={{ color: CATEGORY_COLOR[t.category] }}>{CATEGORY_LABEL[t.category]}</span>
      </span>
    </Link>
  );
}

function TaskPicker({
  linked,
  editable,
  onPick,
  onClose,
}: {
  linked: string[];
  editable: Set<string>;
  onPick: (taskId: string) => void;
  onClose: () => void;
}) {
  const [q, setQ] = useState("");
  const all = useGlobalTasks({});
  const words = q.trim().toLowerCase();
  const list = (all.data ?? []).filter(
    (t) =>
      editable.has(t.project_id) &&
      !linked.includes(t.id) &&
      (!words || `${taskRef(t)} ${t.title} ${t.project_title}`.toLowerCase().includes(words)),
  );
  return (
    <Popover label="Link a task" onClose={onClose} wide>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (list[0]) onPick(list[0].id);
        }}
      >
        <input
          autoFocus
          className="border-rule bg-surface mb-1 min-h-10 w-full rounded-sm px-2 font-mono text-[13px] outline-none focus:border-accent"
          placeholder="find a task or project"
          aria-label="Find a task"
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
      </form>
      {list.slice(0, 40).map((t) => (
        <button
          key={t.id}
          type="button"
          className="hover:bg-surface-raised flex min-h-10 cursor-pointer items-baseline gap-2 rounded-sm px-2 text-left text-sm"
          onClick={() => onPick(t.id)}
        >
          <span className="label-mono text-text-quiet shrink-0 text-[10px]">{taskRef(t)}</span>
          <span className="min-w-0 flex-1 truncate">{t.title}</span>
          <span className="label-mono shrink-0 text-[10px]" style={{ color: projectColor(t.project_color) }}>
            {t.project_title}
          </span>
        </button>
      ))}
      {list.length === 0 && <p className="text-text-quiet px-2 py-2 text-sm">{all.isLoading ? "Loading…" : "No open tasks match."}</p>}
    </Popover>
  );
}
