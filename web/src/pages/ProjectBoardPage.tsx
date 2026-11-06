import { useMemo, useState } from "react";
import type { Status } from "../api/client";
import { Button, ErrorNote } from "../components/ui";
import { Board, type BoardColumn } from "../tasks/Board";
import { CATEGORIES, CATEGORY_COLOR, CATEGORY_LABEL } from "../tasks/format";
import { useCreateStatus, useDeleteStatus, useMoveTask, useProjectTasks, useStatuses, useUpdateStatus } from "../tasks/queries";
import { TaskCard } from "../tasks/TaskCard";
import { useOpenTask } from "../tasks/TaskDrawer";
import { useProject } from "./ProjectPage";

const selectClass = "border-rule bg-surface min-h-10 w-full rounded-md px-2 font-mono text-[12px] outline-none focus:border-accent";

/** The Board tab: the project's own columns, drag cards between them. */
export function ProjectBoardPage() {
  const { detail, newTask } = useProject();
  const projectId = detail.project.id;
  const statuses = useStatuses(projectId);
  const tasks = useProjectTasks(projectId);
  const move = useMoveTask(projectId);
  const [, openTask] = useOpenTask();
  const [editing, setEditing] = useState<string | null>(null);

  const columns: BoardColumn[] = useMemo(() => {
    const list = statuses.data ?? [];
    const all = tasks.data ?? [];
    return list.map((s) => {
      const mine = all.filter((t) => t.status_id === s.id);
      return {
        id: s.id,
        tasks: mine,
        header:
          editing === s.id ? (
            <ColumnEditor status={s} others={list.filter((o) => o.id !== s.id)} count={mine.length} onDone={() => setEditing(null)} />
          ) : (
            <div className="flex items-center gap-2 px-3 pt-3 pb-2">
              <span className="size-2.5 shrink-0 rounded-full" style={{ background: CATEGORY_COLOR[s.category] }} aria-hidden="true" />
              <h2 className="font-display flex-1 truncate text-[15px] font-extrabold">{s.name}</h2>
              <span className="label-mono text-text-quiet text-[11px]">{mine.length}</span>
              <button
                type="button"
                aria-label={`Edit ${s.name} column`}
                onClick={() => setEditing(s.id)}
                className="text-text-quiet hover:text-text size-8 cursor-pointer rounded-sm text-lg leading-none"
              >
                ⋯
              </button>
            </div>
          ),
        footer: (
          <button
            type="button"
            onClick={() => newTask(s.id)}
            className="label-mono text-text-quiet hover:text-text hover:bg-surface-card m-2 mt-0 min-h-10 cursor-pointer rounded-md text-left text-[11px] px-2"
          >
            + Add
          </button>
        ),
      };
    });
  }, [statuses.data, tasks.data, editing, newTask]);

  if (statuses.isError || tasks.isError) return <div className="p-5"><ErrorNote error={statuses.error ?? tasks.error} /></div>;
  if (!statuses.data || !tasks.data) return <p className="label-mono text-text-quiet p-5">Loading…</p>;
  const byId = new Map(statuses.data.map((s) => [s.id, s]));

  return (
    <div className="flex flex-col pt-4 sm:min-h-0 sm:flex-1">
      <Board
        columns={columns}
        onMove={(taskId, columnId, afterId) => move(taskId, byId.get(columnId)!, afterId)}
        renderCard={(t, lifted) => <TaskCard task={t} onOpen={openTask} lifted={lifted} />}
        trailing={<AddColumn projectId={projectId} />}
      />
    </div>
  );
}

function AddColumn({ projectId }: { projectId: string }) {
  const create = useCreateStatus(projectId);
  const [name, setName] = useState("");
  const [category, setCategory] = useState("in_progress");
  const [open, setOpen] = useState(false);
  if (!open)
    return (
      <div className="shrink-0">
        <button
          type="button"
          onClick={() => setOpen(true)}
          className="border-rule label-mono text-text-quiet hover:text-text min-h-11 w-36 shrink-0 cursor-pointer rounded-lg border-dashed text-[11px]"
        >
          + Column
        </button>
      </div>
    );
  return (
    <form
      className="border-rule bg-surface-card flex h-fit w-60 shrink-0 flex-col gap-2 rounded-lg p-3"
      onSubmit={(e) => {
        e.preventDefault();
        create.mutate({ name, category }, { onSuccess: () => (setName(""), setOpen(false)) });
      }}
    >
      <input
        autoFocus
        className={selectClass}
        placeholder="Column name"
        value={name}
        maxLength={40}
        onChange={(e) => setName(e.target.value)}
        aria-label="Column name"
      />
      <CategorySelect value={category} onChange={setCategory} />
      <ErrorNote error={create.error} />
      <div className="flex gap-2">
        <Button type="button" className="min-h-9 flex-1 px-2 text-sm" onClick={() => setOpen(false)}>Cancel</Button>
        <Button type="submit" variant="primary" className="min-h-9 flex-1 px-2 text-sm" disabled={!name.trim()}>Add</Button>
      </div>
    </form>
  );
}

function CategorySelect({ value, onChange }: { value: string; onChange: (c: string) => void }) {
  return (
    <label className="flex flex-col gap-1">
      <span className="label-mono text-text-quiet text-[10px]">Counts as</span>
      <select className={selectClass} value={value} onChange={(e) => onChange(e.target.value)}>
        {CATEGORIES.map((c) => (
          <option key={c} value={c}>{CATEGORY_LABEL[c]}</option>
        ))}
      </select>
    </label>
  );
}

function ColumnEditor({ status, others, count, onDone }: { status: Status; others: Status[]; count: number; onDone: () => void }) {
  const update = useUpdateStatus(status.project_id);
  const remove = useDeleteStatus(status.project_id);
  const [name, setName] = useState(status.name);
  const [moveTo, setMoveTo] = useState(others[0]?.id ?? "");
  const [confirm, setConfirm] = useState(false);
  return (
    <form
      className="flex flex-col gap-2 p-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (name.trim() && name !== status.name) update.mutate({ id: status.id, name }, { onSuccess: onDone });
        else onDone();
      }}
    >
      <input autoFocus className={selectClass} value={name} maxLength={40} onChange={(e) => setName(e.target.value)} aria-label="Column name" />
      <CategorySelect value={status.category} onChange={(category) => update.mutate({ id: status.id, category })} />
      {confirm ? (
        <div className="flex flex-col gap-2">
          {count > 0 && (
            <label className="flex flex-col gap-1">
              <span className="label-mono text-text-quiet text-[10px]">Move its {count} tasks to</span>
              <select className={selectClass} value={moveTo} onChange={(e) => setMoveTo(e.target.value)}>
                {others.map((o) => (
                  <option key={o.id} value={o.id}>{o.name}</option>
                ))}
              </select>
            </label>
          )}
          <Button
            type="button"
            className="text-late min-h-9 border-current text-sm"
            disabled={others.length === 0}
            onClick={() => remove.mutate({ id: status.id, moveTo: count > 0 ? moveTo : undefined }, { onSuccess: onDone })}
          >
            Remove column
          </Button>
        </div>
      ) : null}
      <ErrorNote error={update.error ?? remove.error} />
      <div className="flex gap-2">
        {!confirm && (
          <Button type="button" className="text-late min-h-9 flex-1 px-2 text-sm" disabled={others.length === 0} onClick={() => setConfirm(true)}>
            Remove…
          </Button>
        )}
        <Button type="submit" variant="primary" className="min-h-9 flex-1 px-2 text-sm">Done</Button>
      </div>
    </form>
  );
}
