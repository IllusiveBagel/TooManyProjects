import { useMemo } from "react";
import { useSearchParams } from "react-router";
import { useMe } from "../auth/queries";
import { useCanvas } from "../canvas/queries";
import { ErrorNote } from "../components/ui";
import { Board, type BoardColumn } from "../tasks/Board";
import { CATEGORIES, CATEGORY_COLOR, CATEGORY_LABEL } from "../tasks/format";
import { useGlobalTasks, useMoveGlobal, type GlobalFilter } from "../tasks/queries";
import { TaskCard } from "../tasks/TaskCard";
import { TaskDrawer, useOpenTask } from "../tasks/TaskDrawer";

const selectClass =
  "border-rule bg-surface min-h-10 rounded-md px-2 font-mono text-[12px] outline-none focus:border-accent max-w-[46vw] sm:max-w-none";

/**
 * Every project's tasks on four fixed columns. Each project's own columns
 * fold into these by category; dropping a card moves it to that project's
 * first column of the category.
 */
export function GlobalBoardPage() {
  const [params, setParams] = useSearchParams();
  const me = useMe();
  const filter: GlobalFilter = useMemo(() => {
    const f: GlobalFilter = {};
    const project = params.get("project");
    const family = params.get("family");
    if (project) f.project_id = project;
    if (family) f.family = family;
    if (params.get("mine")) f.mine = true;
    return f;
  }, [params]);
  const tasks = useGlobalTasks(filter);
  const canvas = useCanvas(false);
  const move = useMoveGlobal(filter);
  const [, openTask] = useOpenTask();

  const projects = canvas.data?.projects ?? [];
  const parents = new Set((canvas.data?.links ?? []).filter((l) => l.kind === "spawned").map((l) => l.source_id));
  const families = projects.filter((p) => parents.has(p.id));
  const viewOnly = new Set(projects.filter((p) => p.role === "viewer").map((p) => p.id));
  const shared = projects.some((p) => p.role !== "owner") || (tasks.data ?? []).some((t) => t.assignee_id && t.assignee_id !== me.id);

  const setFilter = (key: "project" | "family", value: string) =>
    setParams((p) => {
      const next = new URLSearchParams(p);
      next.delete("project");
      next.delete("family");
      if (value) next.set(key, value);
      return next;
    });

  const columns: BoardColumn[] = CATEGORIES.map((c) => {
    const mine = (tasks.data ?? []).filter((t) => t.category === c);
    return {
      id: c,
      tasks: mine,
      header: (
        <div className="flex items-center gap-2 px-3 pt-3 pb-2">
          <span className="size-2.5 shrink-0 rounded-full" style={{ background: CATEGORY_COLOR[c] }} aria-hidden="true" />
          <h2 className="font-display flex-1 text-[15px] font-extrabold">{CATEGORY_LABEL[c]}</h2>
          <span className="label-mono text-text-quiet text-[11px]">{mine.length}</span>
        </div>
      ),
    };
  });

  return (
    <div className="relative flex flex-col sm:min-h-0 sm:flex-1">
      <div className="flex flex-wrap items-end gap-x-6 gap-y-3 px-4 pt-4 pb-3 sm:px-5">
        <div>
          <h1 className="font-display text-[30px] leading-tight font-extrabold tracking-[-0.03em]">Everything</h1>
          <p className="text-text-quiet text-sm">
            Open tasks from every active project, plus anything finished in the last two weeks. Late and soonest due sit at the top.
          </p>
        </div>
        <div className="ml-auto flex flex-wrap items-end gap-2">
          {(shared || filter.mine) && (
            <label className="border-rule label-mono text-text-quiet hover:text-text flex min-h-10 cursor-pointer items-center gap-2 rounded-md px-3 text-[11px]">
              <input
                type="checkbox"
                className="accent-[var(--tmp-accent)]"
                checked={!!filter.mine}
                onChange={(e) =>
                  setParams((p) => {
                    const next = new URLSearchParams(p);
                    if (e.target.checked) next.set("mine", "1");
                    else next.delete("mine");
                    return next;
                  })
                }
              />
              My tasks
            </label>
          )}
          <label className="flex flex-col gap-1">
            <span className="label-mono text-text-quiet text-[10px]">Project</span>
            <select className={selectClass} value={params.get("project") ?? ""} onChange={(e) => setFilter("project", e.target.value)}>
              <option value="">All projects</option>
              {projects.map((p) => (
                <option key={p.id} value={p.id}>{p.title}</option>
              ))}
            </select>
          </label>
          <label className="flex flex-col gap-1">
            <span className="label-mono text-text-quiet text-[10px]">Family</span>
            <select className={selectClass} value={params.get("family") ?? ""} onChange={(e) => setFilter("family", e.target.value)}>
              <option value="">Any family</option>
              {families.map((p) => (
                <option key={p.id} value={p.id}>{p.title} and its spawn</option>
              ))}
            </select>
          </label>
        </div>
      </div>
      {tasks.isError ? (
        <div className="p-5"><ErrorNote error={tasks.error} /></div>
      ) : !tasks.data ? (
        <p className="label-mono text-text-quiet p-5">Loading…</p>
      ) : (
        <div className="relative flex flex-col sm:min-h-0 sm:flex-1">
          <Board
            columns={columns}
            reorderable={false}
            onMove={(taskId, category) => move(taskId, category)}
            renderCard={(t, lifted) => <TaskCard task={t} onOpen={openTask} showProject lifted={lifted} />}
            canDrag={(t) => !viewOnly.has(t.project_id)}
          />
          <TaskDrawer />
        </div>
      )}
    </div>
  );
}
