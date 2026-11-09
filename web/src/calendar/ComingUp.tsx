import { useMemo, useState } from "react";
import { Link } from "react-router";
import { useMe } from "../auth/queries";
import { EventDialog } from "./EventDialog";
import { eventLabel, hoursText, initials, live, whenText, type Occurrence } from "./format";
import { useCalendar } from "./queries";

/** A project's next work sessions, on its overview. */
export function ComingUp({ projectId, canPlan }: { projectId: string; canPlan: boolean }) {
  const me = useMe();
  const filter = useMemo(() => {
    const now = new Date();
    now.setSeconds(0, 0);
    return { from: now.toISOString(), to: new Date(now.getTime() + 14 * 86_400_000).toISOString(), project_id: projectId };
  }, [projectId]);
  const list = useCalendar(filter).data ?? [];
  const [open, setOpen] = useState<Occurrence | null>(null);
  if (list.length === 0 && !canPlan) return null;
  return (
    <section className="border-rule bg-surface-card shadow-card min-w-0 rounded-lg p-4">
      <div className="mb-2 flex items-center gap-2">
        <h2 className="label-mono text-text-quiet">Coming up</h2>
        <Link to="/calendar" className="label-mono text-text-quiet hover:text-text ml-auto text-[11px]">Calendar ↗</Link>
        {canPlan && (
          <Link to={`/calendar?plan=${projectId}`} className="label-mono border-rule hover:bg-surface-raised flex min-h-9 items-center rounded-md px-2.5 text-[11px]">
            + Plan session
          </Link>
        )}
      </div>
      {list.length === 0 ? (
        <p className="text-text-quiet text-sm">No sessions in the next two weeks.</p>
      ) : (
        <ul className="flex flex-col">
          {list.slice(0, 6).map((o) => (
            <li key={o.event.id + o.occurrence}>
              <button
                type="button"
                onClick={() => setOpen(o)}
                className="border-rule hover:bg-surface-raised flex min-h-12 w-full cursor-pointer flex-wrap items-center gap-x-3 gap-y-1 border-x-0 border-t-0 px-1 py-1.5 text-left"
              >
                <span className="w-44 shrink-0 font-mono text-[12px]">{whenText(o)}</span>
                <span className="min-w-0 flex-1 truncate font-bold">{o.event.title ? eventLabel(o.event) : "Work session"}</span>
                <span className="label-mono text-text-quiet text-[10px]">{hoursText(o.event.minutes)}</span>
                <span className="flex items-center">
                  {[o.event.user_name, ...o.joiners.map((j) => j.name)].slice(0, 4).map((n, i) => (
                    <span
                      key={n + i}
                      title={n}
                      className="label-mono bg-surface-raised border-line grid size-6 place-items-center rounded-full border-[1.5px] text-[8.5px] font-semibold"
                      style={{ marginLeft: i ? -6 : 0 }}
                    >
                      {initials(n)}
                    </span>
                  ))}
                </span>
                {o.can_join && (
                  <span className={`label-mono text-[10px] ${o.joined ? "text-accent" : "text-text-quiet"}`}>
                    {o.joined ? "You're in" : o.event.user_id === me.id ? "" : "Join?"}
                  </span>
                )}
              </button>
            </li>
          ))}
        </ul>
      )}
      {open && <EventDialog occurrence={live(open, list)!} onClose={() => setOpen(null)} />}
    </section>
  );
}
