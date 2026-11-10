import { useQueryClient } from "@tanstack/react-query";
import { useState, type ReactNode } from "react";
import { Link } from "react-router";
import type { Task } from "../api/client";
import { useUpdateProject } from "../canvas/queries";
import { EventDialog } from "../calendar/EventDialog";
import { clock, eventLabel, hoursText, live, type Occurrence } from "../calendar/format";
import { Button, ErrorNote } from "../components/ui";
import { CATEGORY_COLOR } from "../tasks/format";
import { useUpdateTask } from "../tasks/queries";
import { TaskDrawer, useOpenTask } from "../tasks/TaskDrawer";
import { projectColor } from "../theme/resolve";
import { dueText, findUrl, longDate, sessionsHeading, shortUrl, type Today } from "../today/format";
import { useQuickAddUi } from "../today/QuickAdd";
import { usePalette } from "../today/Palette";
import { useToday } from "../today/queries";
import { money, plural } from "../parts/format";

const SEARCH_ICON = "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zM20 20l-4-4";

function Section({ title, count, color, children }: { title: string; count?: number | string; color?: string; children: ReactNode }) {
  return (
    <section className="border-rule bg-surface-card shadow-card min-w-0 overflow-hidden rounded-lg">
      <h2 className="label-mono border-rule flex items-center gap-2.5 border-x-0 border-t-0 px-4 py-3.5 text-[13px] tracking-[0.12em]">
        <span style={{ color: color ?? "var(--tmp-text-quiet)" }}>{title}</span>
        {count !== undefined && <span className="text-text-quiet">{count}</span>}
      </h2>
      <ul>{children}</ul>
    </section>
  );
}

const rowClass = "border-rule flex min-h-16 items-center gap-3 border-x-0 border-t-0 px-4 py-2 last:border-b-0";

/** A task line: tick it done, or open it in the drawer. */
function TaskRow({ task, today, show }: { task: Task; today: string; show: "column" | "due" }) {
  const [, openTask] = useOpenTask();
  const update = useUpdateTask();
  const [ticked, setTicked] = useState(false);
  const right =
    show === "due"
      ? task.due_date && dueText(task.due_date, today)
      : [
          task.status_name,
          task.category === "blocked" && task.due_date ? `Due ${dueText(task.due_date, today)}` : null,
          task.checklist_total > 0 ? `${task.checklist_done}/${task.checklist_total}` : null,
        ]
          .filter(Boolean)
          .join(" · ");
  const late = task.overdue;
  return (
    <li className={rowClass}>
      <input
        type="checkbox"
        aria-label={`Mark ${task.title} done`}
        checked={ticked}
        disabled={ticked}
        onChange={() => {
          setTicked(true);
          update.mutate({ id: task.id, category: "done" }, { onError: () => setTicked(false) });
        }}
        className="accent-accent size-6 shrink-0 cursor-pointer"
      />
      <button type="button" onClick={() => openTask(task.id)} className={`min-w-0 flex-1 cursor-pointer text-left ${ticked ? "line-through opacity-60" : ""}`}>
        <span className="block truncate text-[17px] font-bold">{task.title}</span>
        <span className="label-mono block truncate text-[11px]" style={{ color: projectColor(task.project_color) }}>
          {task.project_title}
        </span>
      </button>
      <span className={`label-mono shrink-0 text-right text-[11px] ${late && show === "due" ? "text-late" : "text-text-quiet"}`}>
        {right}
      </span>
    </li>
  );
}

function Sessions({ data }: { data: Today }) {
  const [open, setOpen] = useState<Occurrence | null>(null);
  if (data.sessions.length === 0) return null;
  const starts = data.sessions.map((o) => new Date(o.start_at));
  const minutes = data.sessions.reduce((n, o) => n + o.event.minutes, 0);
  const now = Date.now();
  return (
    <Section title={sessionsHeading(starts)} count={hoursText(minutes)} color="var(--tmp-accent)">
      {data.sessions.map((o) => {
        const past = new Date(o.end_at).getTime() < now;
        return (
          <li key={o.event.id + o.occurrence} className={rowClass}>
            <button type="button" onClick={() => setOpen(o)} className={`flex min-w-0 flex-1 cursor-pointer items-center gap-4 text-left ${past ? "opacity-55" : ""}`}>
              <span className="shrink-0 font-mono text-[14px]">
                {clock(new Date(o.start_at))}–{clock(new Date(o.end_at))}
              </span>
              <span className="h-9 w-1 shrink-0 rounded-full" style={{ background: projectColor(o.event.project_color ?? "slot-1") }} aria-hidden="true" />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-[17px] font-bold">{eventLabel(o.event)}</span>
                {o.event.title && o.event.project_title && (
                  <span className="label-mono text-text-quiet block truncate text-[11px]">{o.event.project_title}</span>
                )}
              </span>
              <span className="label-mono text-text-quiet shrink-0 text-[11px]">{hoursText(o.event.minutes)}</span>
            </button>
          </li>
        );
      })}
      {open && <EventDialog occurrence={live(open, data.sessions)!} onClose={() => setOpen(null)} />}
    </Section>
  );
}

function Quiet({ data }: { data: Today }) {
  const update = useUpdateProject();
  const qc = useQueryClient();
  if (data.quiet.length === 0) return null;
  return (
    <Section title="Gone quiet" count={data.quiet.length}>
      {data.quiet.map((p) => (
        <li key={p.id} className={rowClass}>
          <span className="size-3 shrink-0 rounded-[3px]" style={{ background: projectColor(p.color) }} aria-hidden="true" />
          <Link to={`/projects/${p.id}`} className="min-w-0 flex-1 truncate text-[17px] font-bold hover:underline">
            {p.title}
          </Link>
          <span className="label-mono text-text-quiet shrink-0 text-[11px]">
            {p.total_tasks === 0 ? "No tasks yet" : `${p.quiet_days} days`}
          </span>
          <Button
            className="min-h-10 shrink-0 px-3 text-sm"
            disabled={update.isPending && update.variables?.id === p.id}
            onClick={() => update.mutate({ id: p.id, status: "paused" }, { onSuccess: () => qc.invalidateQueries({ queryKey: ["today"] }) })}
            title="Mark it paused, so it stops showing here"
          >
            Pause
          </Button>
        </li>
      ))}
    </Section>
  );
}

/** A link to the Shopping list, when there's something to buy. */
function Shopping({ data }: { data: Today }) {
  const s = data.shopping;
  if (s.need_count === 0) return null;
  return (
    <Link
      to="/shopping"
      className="border-rule bg-surface-card shadow-card hover:bg-surface-raised flex min-h-16 items-center gap-3 rounded-lg px-4 py-3"
    >
      <span className="min-w-0 flex-1">
        <span className="label-mono text-text-quiet block text-[13px] tracking-[0.12em]">Shopping list</span>
        <span className="block truncate text-[17px] font-bold">{plural(s.need_count, "part")} to buy</span>
      </span>
      <span className="shrink-0 font-mono text-[18px]">{money(s.to_buy, s.currency)}</span>
      <span className="text-text-quiet shrink-0" aria-hidden="true">→</span>
    </Link>
  );
}

function Inbox({ data }: { data: Today }) {
  const start = useQuickAddUi((s) => s.start);
  if (data.inbox.length === 0) return null;
  return (
    <Section title="Inbox" count={data.inbox.length}>
      {data.inbox.map((n) => {
        const url = findUrl(n.excerpt);
        return (
          <li key={n.id} className={rowClass}>
            <Link to={`/notes/${n.id}`} className="min-w-0 flex-1">
              <span className="block truncate text-[17px] font-bold">{n.title}</span>
              <span className="label-mono text-text-quiet block truncate text-[11px]">
                Inbox{url ? ` · ${shortUrl(url)}` : n.excerpt && n.excerpt !== n.title ? ` · ${n.excerpt}` : ""}
              </span>
            </Link>
            <Button className="min-h-10 shrink-0 px-3 text-sm" onClick={() => start({ inbox: n, url: null })}>
              File it
            </Button>
          </li>
        );
      })}
    </Section>
  );
}

/** What to touch next: in progress, stuck, due, tonight's sessions, quiet projects and the Inbox. */
export function TodayPage() {
  const today = useToday();
  const startAdd = useQuickAddUi((s) => s.start);
  const openPalette = usePalette((s) => s.show);
  const data = today.data;
  const empty =
    data &&
    [data.now, data.blocked, data.due, data.sessions, data.quiet, data.inbox].every((l) => l.length === 0);

  return (
    <div className="mx-auto flex w-full max-w-[1400px] flex-col gap-5 px-4 pt-5 pb-10 sm:px-8 sm:pt-8">
      <header className="flex flex-wrap items-end gap-3">
        <div className="mr-auto">
          <p className="label-mono text-text-quiet text-[12px] tracking-[0.12em]">{data ? longDate(data.date) : " "}</p>
          <h1 className="font-display text-[44px] leading-[1.05] font-extrabold tracking-[-0.03em] sm:text-[52px]">Today</h1>
        </div>
        <div className="hidden items-center gap-3 sm:flex">
          <Button className="border-line min-h-12 border px-4" onClick={openPalette}>
            <svg viewBox="0 0 24 24" className="size-4.5" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
              <path d={SEARCH_ICON} />
            </svg>
            Search
            <kbd className="label-mono text-text-quiet text-[10px]">Ctrl K</kbd>
          </Button>
          <Button variant="primary" className="min-h-12 px-4" onClick={() => startAdd()}>
            + Quick add
            <kbd className="label-mono text-[10px] opacity-75">Alt A</kbd>
          </Button>
        </div>
      </header>
      <ErrorNote error={today.error} />
      {!data && !today.error && <p className="label-mono text-text-quiet">Loading…</p>}
      {empty && (
        <div className="border-rule max-w-xl rounded-lg border-dashed p-5">
          <h2 className="font-display text-lg font-extrabold">A clear day</h2>
          <p className="text-text-quiet mt-1">
            Nothing is in progress, blocked or due this week. Drag a card to Doing on a board, or{" "}
            <button type="button" className="text-accent cursor-pointer underline" onClick={() => startAdd()}>
              add something
            </button>
            .
          </p>
        </div>
      )}
      {data && empty && <div className="max-w-xl"><Shopping data={data} /></div>}
      {data && !empty && (
        <div className="grid grid-cols-1 items-start gap-5 lg:grid-cols-[minmax(0,1.2fr)_minmax(0,1fr)]">
          {/* On a phone, tonight's sessions come first; on a computer they head the right column. */}
          <div className="flex min-w-0 flex-col gap-5">
            <div className="lg:hidden">
              <Sessions data={data} />
            </div>
            {data.now.length > 0 && (
              <Section title="Now" count={data.now.length} color={CATEGORY_COLOR.in_progress}>
                {data.now.map((t) => <TaskRow key={t.id} task={t} today={data.date} show="column" />)}
              </Section>
            )}
            {data.blocked.length > 0 && (
              <Section title="Blocked" count={data.blocked.length} color="var(--tmp-late)">
                {data.blocked.map((t) => <TaskRow key={t.id} task={t} today={data.date} show="column" />)}
              </Section>
            )}
            <Inbox data={data} />
          </div>
          <div className="flex min-w-0 flex-col gap-5">
            <div className="hidden lg:block">
              <Sessions data={data} />
            </div>
            {data.due.length > 0 && (
              <Section title={data.due.some((t) => t.overdue) ? "Late and due in 7 days" : "Due in the next 7 days"} count={data.due.length}>
                {data.due.map((t) => <TaskRow key={t.id} task={t} today={data.date} show="due" />)}
              </Section>
            )}
            <Quiet data={data} />
            <Shopping data={data} />
          </div>
        </div>
      )}
      <TaskDrawer />
    </div>
  );
}
