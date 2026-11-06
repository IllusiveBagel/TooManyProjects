import type { Dashboard } from "../api/client";
import { dueLabel, lastActive } from "../canvas/format";
import { ErrorNote, ViaBadge } from "../components/ui";
import { activityText, CATEGORY_COLOR, PRIORITY_LABEL, taskRef } from "../tasks/format";
import { useDashboard } from "../tasks/queries";
import { useOpenTask } from "../tasks/TaskDrawer";
import { useProject } from "./ProjectPage";

/** The Overview tab: stat tiles, the weekly chart, where tasks sit, up next, activity. */
export function ProjectOverview() {
  const { detail, newTask } = useProject();
  const dash = useDashboard(detail.project.id);
  const [, openTask] = useOpenTask();
  const { stats } = detail.project;

  return (
    <div className="flex flex-col gap-4 p-4 sm:overflow-y-auto sm:p-5">
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <Tile label="Done" value={`${stats.done_tasks}`} sub={`of ${stats.total_tasks} tasks`} />
        <Tile label="In progress" value={`${stats.in_progress_tasks}`} sub={`${stats.open_tasks} open in all`} />
        <Tile label="Late" value={`${stats.overdue_tasks}`} sub="past their due date" tone={stats.overdue_tasks > 0 ? "text-late" : ""} />
        <Tile label="Last touched" value={lastActive(stats.last_active_at)} sub={new Date(stats.last_active_at).toLocaleDateString("en-GB", { day: "numeric", month: "short" })} />
      </div>

      {dash.isError && <ErrorNote error={dash.error} />}
      {dash.data && (
        <>
          <div className="grid grid-cols-1 gap-4 lg:grid-cols-[3fr_2fr]">
            <Card title="Shipped per week">
              <WeeklyChart weekly={dash.data.weekly} />
            </Card>
            <Card title="Where tasks sit">
              <Spread columns={dash.data.columns} />
            </Card>
          </div>
          <div className="grid grid-cols-1 gap-4 lg:grid-cols-[3fr_2fr]">
            <Card title="Up next">
              {dash.data.up_next.length === 0 ? (
                <p className="text-text-quiet text-sm">
                  Nothing open.{" "}
                  <button type="button" className="text-accent cursor-pointer font-bold" onClick={() => newTask()}>
                    Add a task
                  </button>
                </p>
              ) : (
                <ul className="flex flex-col">
                  {dash.data.up_next.map((t) => (
                    <li key={t.id}>
                      <button
                        type="button"
                        onClick={() => openTask(t.id)}
                        className="border-rule hover:bg-surface-raised flex min-h-12 w-full cursor-pointer items-center gap-3 border-x-0 border-t-0 px-1 text-left"
                      >
                        <span className="size-2.5 shrink-0 rounded-full" style={{ background: CATEGORY_COLOR[t.category] }} aria-hidden="true" />
                        <span className="label-mono text-text-quiet w-11 shrink-0 text-[10px]">{taskRef(t)}</span>
                        <span className="flex-1 truncate font-bold">{t.title}</span>
                        {t.priority && <span className="label-mono text-text-quiet text-[10px]">{PRIORITY_LABEL[t.priority]}</span>}
                        {t.due_date && (
                          <span className={`label-mono text-[10px] ${t.overdue ? "text-late font-semibold" : "text-text-quiet"}`}>
                            {dueLabel(t.due_date)}
                          </span>
                        )}
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </Card>
            <Card title="Activity">
              <ActivityList items={dash.data.activity} onOpen={openTask} />
            </Card>
          </div>
        </>
      )}
    </div>
  );
}

function Card({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="border-rule bg-surface-card shadow-card min-w-0 rounded-lg p-4">
      <h2 className="label-mono text-text-quiet mb-3">{title}</h2>
      {children}
    </section>
  );
}

function Tile({ label, value, sub, tone = "" }: { label: string; value: string; sub: string; tone?: string }) {
  return (
    <div className="border-rule bg-surface-card shadow-card rounded-lg px-4 py-3">
      <span className="label-mono text-text-quiet block">{label}</span>
      <span className={`font-display block text-[32px] leading-tight font-extrabold tracking-[-0.03em] ${tone}`}>{value}</span>
      <span className="text-text-quiet text-xs">{sub}</span>
    </div>
  );
}

const shortDate = (iso: string) => {
  const [y, m, d] = iso.split("-").map(Number) as [number, number, number];
  return new Date(y, m - 1, d).toLocaleDateString("en-GB", { day: "numeric", month: "short" });
};

/** One series of bars: tasks finished each week. Hover or focus a bar for its value. */
function WeeklyChart({ weekly }: { weekly: Dashboard["weekly"] }) {
  const max = Math.max(1, ...weekly.map((w) => w.done));
  const total = weekly.reduce((n, w) => n + w.done, 0);
  return (
    <figure className="flex flex-col gap-2">
      <figcaption className="text-text-quiet text-xs">
        {total} finished in the last {weekly.length} weeks
      </figcaption>
      <div className="border-rule flex h-36 items-end gap-1.5 border-x-0 border-t-0" role="list">
        {weekly.map((w, i) => {
          const label = `Week of ${shortDate(w.week)}: ${w.done} done`;
          const last = i === weekly.length - 1;
          return (
            <div
              key={w.week}
              role="listitem"
              tabIndex={0}
              aria-label={label}
              title={label}
              className="group relative flex h-full flex-1 cursor-default items-end justify-center outline-none"
            >
              <span className="label-mono bg-text text-surface pointer-events-none absolute -top-1 z-10 hidden -translate-y-full rounded-sm px-1.5 py-0.5 text-[10px] whitespace-nowrap group-hover:block group-focus:block">
                {label}
              </span>
              <span
                className={`w-full max-w-9 rounded-t-[4px] ${last ? "bg-accent" : "bg-accent/60"} group-hover:bg-accent`}
                style={{ height: w.done === 0 ? 2 : `${(w.done / max) * 100}%`, opacity: w.done === 0 ? 0.35 : 1 }}
              />
            </div>
          );
        })}
      </div>
      <div className="label-mono text-text-quiet flex justify-between text-[10px]">
        <span>{shortDate(weekly[0]!.week)}</span>
        <span>This week · {weekly.at(-1)!.done}</span>
      </div>
    </figure>
  );
}

/** A single stacked bar of tasks per column, with a labelled legend underneath. */
function Spread({ columns }: { columns: Dashboard["columns"] }) {
  const total = columns.reduce((n, c) => n + c.count, 0);
  if (total === 0) return <p className="text-text-quiet text-sm">No tasks yet.</p>;
  return (
    <div className="flex flex-col gap-3">
      <div className="flex h-5 gap-0.5 overflow-hidden rounded-[4px]" role="img" aria-label={columns.map((c) => `${c.name} ${c.count}`).join(", ")}>
        {columns
          .filter((c) => c.count > 0)
          .map((c) => (
            <span
              key={c.status_id}
              title={`${c.name}: ${c.count}`}
              style={{ flexGrow: c.count, background: CATEGORY_COLOR[c.category] }}
            />
          ))}
      </div>
      <ul className="flex flex-col gap-1.5 text-sm">
        {columns.map((c) => (
          <li key={c.status_id} className="flex items-center gap-2">
            <span className="size-2.5 shrink-0 rounded-sm" style={{ background: CATEGORY_COLOR[c.category] }} aria-hidden="true" />
            <span className="flex-1">{c.name}</span>
            <span className="font-mono text-[13px]">{c.count}</span>
            <span className="label-mono text-text-quiet w-10 text-right text-[10px]">{Math.round((c.count / total) * 100)}%</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

function ActivityList({ items, onOpen }: { items: Dashboard["activity"]; onOpen: (id: string) => void }) {
  if (items.length === 0) return <p className="text-text-quiet text-sm">Nothing yet.</p>;
  return (
    <ol className="flex flex-col gap-2 text-sm">
      {items.map((a) => (
        <li key={a.id} className="flex gap-2">
          <span className="label-mono text-text-quiet w-16 shrink-0 text-[10px] leading-5">{lastActive(a.created_at)}</span>
          {a.task_id && a.kind !== "task.deleted" ? (
            <button type="button" className="hover:text-accent cursor-pointer text-left" onClick={() => onOpen(a.task_id!)}>
              <b>{a.user_name ?? "Someone"}</b> {activityText(a)}
                    <ViaBadge via={a.via} />
            </button>
          ) : (
            <span>
              <b>{a.user_name ?? "Someone"}</b> {activityText(a)}
                    <ViaBadge via={a.via} />
            </span>
          )}
        </li>
      ))}
    </ol>
  );
}
