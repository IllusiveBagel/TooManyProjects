import { useState } from "react";
import { Link, NavLink, Outlet, useOutletContext, useParams } from "react-router";
import type { ProjectDetail } from "../api/client";
import { STATUS_LABEL, STATUS_TONE } from "../canvas/format";
import { useProjectDetail } from "../canvas/queries";
import { Modal } from "../components/Modal";
import { Button, ErrorNote } from "../components/ui";
import { projectColor } from "../theme/resolve";
import { useCreateTask, useStatuses } from "../tasks/queries";
import { TaskDrawer, useOpenTask } from "../tasks/TaskDrawer";

const tabClass = ({ isActive }: { isActive: boolean }) =>
  `label-mono border-b-2 px-1 py-2.5 text-xs ${isActive ? "text-text border-accent" : "text-text-quiet border-transparent hover:text-text"}`;

export interface ProjectContext {
  detail: ProjectDetail;
  /** Opens the New task dialog, optionally for one column. */
  newTask: (statusId?: string) => void;
}

export const useProject = () => useOutletContext<ProjectContext>();

/** A project's own page: the header and tabs, with Overview or Board below. */
export function ProjectPage() {
  const { id = "" } = useParams();
  const detail = useProjectDetail(id);
  const [newTaskFor, setNewTaskFor] = useState<string | null | undefined>(undefined);

  if (detail.isError) return <div className="p-5"><ErrorNote error={detail.error} /></div>;
  if (!detail.data) return <p className="label-mono text-text-quiet p-5">Loading…</p>;
  const { project, parents } = detail.data;
  const color = projectColor(project.color);

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <header className="border-t-8 px-4 pt-3 sm:px-5" style={{ borderTopColor: color }}>
        <nav aria-label="Family" className="label-mono text-text-quiet flex flex-wrap items-center gap-1.5 text-[11px]">
          <Link to="/" className="hover:text-text">Canvas</Link>
          {parents.map((p) => (
            <span key={p.id} className="flex items-center gap-1.5">
              <span aria-hidden="true">›</span>
              <Link to={`/projects/${p.id}`} className="hover:text-text">{p.title}</Link>
            </span>
          ))}
          <span aria-hidden="true">›</span>
          <span className="text-text">{project.title}</span>
        </nav>
        <div className="mt-1 flex flex-wrap items-center gap-x-4 gap-y-2">
          <h1 className="font-display text-[34px] leading-tight font-extrabold tracking-[-0.03em] sm:text-[40px]">
            {project.title}
          </h1>
          <span className={`label-mono border-rule rounded-sm border-current px-2 py-1 text-[11px] ${STATUS_TONE[project.status]}`}>
            {STATUS_LABEL[project.status]}
          </span>
          <Button variant="primary" className="ml-auto min-h-10 px-3 text-sm" onClick={() => setNewTaskFor(null)}>
            + Task
          </Button>
        </div>
        {project.summary && <p className="font-reading text-text-quiet mt-1 text-[16px]">{project.summary}</p>}
        <nav aria-label="Project" className="border-rule mt-3 flex gap-5 border-x-0 border-t-0">
          <NavLink to={`/projects/${id}`} end className={tabClass}>Overview</NavLink>
          <NavLink to={`/projects/${id}/board`} className={tabClass}>Board</NavLink>
        </nav>
      </header>
      <div className="relative flex min-h-0 flex-1 flex-col">
        <Outlet context={{ detail: detail.data, newTask: (s?: string) => setNewTaskFor(s ?? null) } satisfies ProjectContext} />
        <TaskDrawer />
      </div>
      {newTaskFor !== undefined && (
        <NewTaskDialog projectId={id} statusId={newTaskFor} onClose={() => setNewTaskFor(undefined)} />
      )}
    </div>
  );
}

function NewTaskDialog({ projectId, statusId, onClose }: { projectId: string; statusId: string | null; onClose: () => void }) {
  const statuses = useStatuses(projectId).data ?? [];
  const create = useCreateTask();
  const [, openTask] = useOpenTask();
  const [title, setTitle] = useState("");
  const [column, setColumn] = useState(statusId ?? "");
  const [due, setDue] = useState("");
  const submit = (andOpen: boolean) =>
    create.mutate(
      { projectId, title, status_id: column || null, due_date: due || null },
      {
        onSuccess: (task) => {
          onClose();
          if (andOpen) openTask(task.id);
        },
      },
    );
  return (
    <Modal title="New task" onClose={onClose}>
      <form
        className="flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          submit(false);
        }}
      >
        <label className="flex flex-col gap-1.5">
          <span className="label-mono text-text-quiet">Title</span>
          <input
            className="border-rule bg-surface min-h-11 rounded-md px-3 outline-none focus:border-accent"
            value={title}
            maxLength={300}
            required
            onChange={(e) => setTitle(e.target.value)}
          />
        </label>
        <div className="grid grid-cols-2 gap-3">
          <label className="flex flex-col gap-1.5">
            <span className="label-mono text-text-quiet">Column</span>
            <select
              className="border-rule bg-surface min-h-11 rounded-md px-3 font-mono text-[13px] outline-none focus:border-accent"
              value={column}
              onChange={(e) => setColumn(e.target.value)}
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
              className="border-rule bg-surface min-h-11 rounded-md px-3 font-mono text-[13px] outline-none focus:border-accent"
              value={due}
              onChange={(e) => setDue(e.target.value)}
            />
          </label>
        </div>
        <ErrorNote error={create.error} />
        <div className="flex flex-wrap justify-end gap-2">
          <Button type="button" onClick={onClose}>Cancel</Button>
          <Button type="button" disabled={!title.trim() || create.isPending} onClick={() => submit(true)}>
            Add and open
          </Button>
          <Button type="submit" variant="primary" disabled={!title.trim() || create.isPending}>
            Add task
          </Button>
        </div>
      </form>
    </Modal>
  );
}
