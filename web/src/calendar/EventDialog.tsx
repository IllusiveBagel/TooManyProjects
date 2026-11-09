import { useState } from "react";
import { Link } from "react-router";
import { useMe } from "../auth/queries";
import { useCanvas } from "../canvas/queries";
import { Modal } from "../components/Modal";
import { Button, ErrorNote } from "../components/ui";
import { projectColor } from "../theme/resolve";
import {
  DAY_NAMES,
  WEEKDAYS,
  WEEKENDS,
  browserZone,
  clock,
  dayBit,
  dayKey,
  eventLabel,
  hoursText,
  initials,
  repeatText,
  whenText,
  type Occurrence,
} from "./format";
import { useCreateEvent, useDeleteEvent, useJoin, useUpdateEvent } from "./queries";

/** A new block, from a drag on the week or a Plan button. */
export interface Draft {
  start: Date;
  end: Date;
  kind?: "session" | "busy";
  projectId?: string;
}

const inputClass =
  "border-rule bg-surface text-text min-h-11 w-full rounded-md px-3 font-mono text-[13px] outline-none focus:border-accent";

/**
 * Plan time, change a block you may edit, or look at someone else's session
 * and say you'll join.
 */
export function EventDialog({
  draft,
  occurrence,
  onClose,
}: {
  draft?: Draft;
  occurrence?: Occurrence;
  onClose: () => void;
}) {
  if (occurrence && !occurrence.event.can_edit) return <SessionView occ={occurrence} onClose={onClose} />;
  return <EventForm draft={draft} occurrence={occurrence} onClose={onClose} />;
}

function minutesBetween(from: string, to: string): number {
  const [fh, fm] = from.split(":").map(Number) as [number, number];
  const [th, tm] = to.split(":").map(Number) as [number, number];
  let m = th * 60 + tm - (fh * 60 + fm);
  if (m <= 0) m += 24 * 60;
  return m;
}

function addMinutes(time: string, minutes: number): string {
  const [h, m] = time.split(":").map(Number) as [number, number];
  const total = (((h * 60 + m + minutes) % 1440) + 1440) % 1440;
  return `${String(Math.floor(total / 60)).padStart(2, "0")}:${String(total % 60).padStart(2, "0")}`;
}

function EventForm({ draft, occurrence, onClose }: { draft?: Draft; occurrence?: Occurrence; onClose: () => void }) {
  const projects = (useCanvas(false).data?.projects ?? []).filter(
    (p) => p.role !== "viewer" && p.status !== "archived",
  );
  const e = occurrence?.event;
  const [kind, setKind] = useState<"session" | "busy">((e?.kind as "session" | "busy") ?? draft?.kind ?? "session");
  const [projectId, setProjectId] = useState(e?.project_id ?? draft?.projectId ?? "");
  const [title, setTitle] = useState(e?.title ?? "");
  const [day, setDay] = useState(e ? e.start_local.slice(0, 10) : dayKey(draft!.start));
  const [from, setFrom] = useState(e ? e.start_local.slice(11, 16) : clock(draft!.start));
  const [to, setTo] = useState(
    e ? addMinutes(e.start_local.slice(11, 16), e.minutes) : clock(draft!.end),
  );
  const [repeat, setRepeat] = useState(e?.repeat_days ?? 0);
  const [until, setUntil] = useState(e?.repeat_until ?? "");
  const create = useCreateEvent();
  const update = useUpdateEvent();
  const remove = useDeleteEvent();
  const busy = create.isPending || update.isPending || remove.isPending;
  const repeats = !!e && e.repeat_days !== 0;

  const save = async (ev: React.FormEvent) => {
    ev.preventDefault();
    const body = {
      title: title.trim() || null,
      start_local: `${day}T${from}`,
      minutes: minutesBetween(from, to),
      repeat_days: repeat,
      repeat_until: repeat && until ? until : null,
    };
    if (e) {
      await update.mutateAsync({ id: e.id, body: { ...body, ...(kind === "session" ? { project_id: projectId } : {}) } });
    } else {
      await create.mutateAsync({
        ...body,
        kind,
        project_id: kind === "session" ? projectId : null,
        tz: browserZone(),
      });
    }
    onClose();
  };

  const del = async (occurrenceOnly: boolean) => {
    if (!e) return;
    await remove.mutateAsync({ id: e.id, occurrence: occurrenceOnly ? occurrence!.occurrence : undefined });
    onClose();
  };

  const toggleDay = (bit: number) => setRepeat((r) => r ^ bit);
  const preset = (mask: number) => setRepeat(mask);
  const projectPicked = kind === "busy" || !!projectId;

  return (
    <Modal title={e ? (e.kind === "busy" ? "Busy time" : "Work session") : "Plan time"} onClose={onClose}>
      <form className="flex flex-col gap-4" onSubmit={save}>
        {!e && (
          <div role="radiogroup" aria-label="Kind" className="border-rule flex overflow-hidden rounded-md">
            {(["session", "busy"] as const).map((k) => (
              <label
                key={k}
                className={`label-mono flex min-h-11 flex-1 cursor-pointer items-center justify-center text-[11px] ${kind === k ? "bg-accent text-on-accent font-semibold" : "text-text-quiet hover:text-text"}`}
              >
                <input type="radio" name="kind" className="sr-only" checked={kind === k} onChange={() => setKind(k)} />
                {k === "session" ? "Work on a project" : "I'm busy"}
              </label>
            ))}
          </div>
        )}
        {kind === "session" && (
          <label className="flex flex-col gap-1.5">
            <span className="label-mono text-text-quiet">Project</span>
            <select className={inputClass} value={projectId} onChange={(ev) => setProjectId(ev.target.value)} required>
              <option value="">Pick a project…</option>
              {projects.map((p) => (
                <option key={p.id} value={p.id}>{p.title}{p.role !== "owner" ? ` (${p.owner_name}'s)` : ""}</option>
              ))}
            </select>
          </label>
        )}
        <label className="flex flex-col gap-1.5">
          <span className="label-mono text-text-quiet">{kind === "session" ? "Focus (optional)" : "What is it"}</span>
          <input
            className={inputClass}
            value={title}
            maxLength={200}
            placeholder={kind === "session" ? "Rear brakes" : "Work"}
            onChange={(ev) => setTitle(ev.target.value)}
          />
        </label>
        <div className="grid grid-cols-[1.4fr_1fr_1fr] gap-2.5">
          <label className="flex min-w-0 flex-col gap-1.5">
            <span className="label-mono text-text-quiet">{repeats || repeat ? "Starts" : "Day"}</span>
            <input type="date" className={inputClass} value={day} required onChange={(ev) => setDay(ev.target.value)} />
          </label>
          <label className="flex min-w-0 flex-col gap-1.5">
            <span className="label-mono text-text-quiet">From</span>
            <input type="time" step={900} className={inputClass} value={from} required onChange={(ev) => setFrom(ev.target.value)} />
          </label>
          <label className="flex min-w-0 flex-col gap-1.5">
            <span className="label-mono text-text-quiet">To</span>
            <input type="time" step={900} className={inputClass} value={to} required onChange={(ev) => setTo(ev.target.value)} />
          </label>
        </div>
        <fieldset className="flex flex-col gap-2">
          <legend className="label-mono text-text-quiet mb-2">Repeat every week on</legend>
          <div className="grid grid-cols-7 gap-1">
            {DAY_NAMES.map((d, i) => {
              const on = (repeat & (1 << i)) !== 0;
              return (
                <button
                  key={d}
                  type="button"
                  aria-pressed={on}
                  aria-label={d}
                  onClick={() => toggleDay(1 << i)}
                  className={`border-rule min-h-11 cursor-pointer rounded-md font-mono text-[12px] ${on ? "bg-accent text-on-accent border-accent font-semibold" : "text-text-quiet hover:text-text"}`}
                >
                  {d[0]}
                </button>
              );
            })}
          </div>
          <div className="flex flex-wrap items-center gap-1.5">
            {[
              ["Weekdays", WEEKDAYS],
              ["Weekends", WEEKENDS],
              ["Just once", 0],
            ].map(([label, mask]) => (
              <button
                key={label}
                type="button"
                onClick={() => preset(mask as number)}
                className={`label-mono border-rule min-h-9 cursor-pointer rounded-md px-2.5 text-[10.5px] ${repeat === mask ? "text-accent border-current" : "text-text-quiet hover:text-text"}`}
              >
                {label}
              </button>
            ))}
            {repeat !== 0 && (
              <label className="ml-auto flex items-center gap-2">
                <span className="label-mono text-text-quiet text-[10px]">Until</span>
                <input type="date" className={`${inputClass} min-h-9 w-auto`} value={until} min={day} onChange={(ev) => setUntil(ev.target.value)} />
              </label>
            )}
          </div>
          {repeat !== 0 && !(repeat & dayBit(new Date(`${day}T00:00`))) && (
            <p className="text-text-quiet text-xs">Starts on the first ticked day from {day}.</p>
          )}
        </fieldset>
        {kind === "busy" ? (
          <p className="border-rule text-text-quiet rounded-md px-3 py-2.5 text-[13px] leading-snug">
            <b className="text-text">Only you see busy time.</b> It counts against your free hours for the week.
          </p>
        ) : (
          !e && (
            <p className="text-text-quiet text-[13px] leading-snug">
              Everyone who can see the project sees this session and can say they'll join.
            </p>
          )
        )}
        {occurrence && e?.kind === "session" && <People occ={occurrence} />}
        {repeats && <p className="text-text-quiet text-xs">Changes apply to every week ({repeatText(e!.repeat_days)}).</p>}
        <ErrorNote error={create.error ?? update.error ?? remove.error} />
        <div className="flex flex-wrap items-center gap-2">
          {e &&
            (repeats ? (
              <>
                <Button type="button" className="!text-late" disabled={busy} onClick={() => del(true)}>Delete this one</Button>
                <Button type="button" className="!text-late" disabled={busy} onClick={() => del(false)}>Every week</Button>
              </>
            ) : (
              <Button type="button" className="!text-late" disabled={busy} onClick={() => del(false)}>Delete</Button>
            ))}
          {e?.project_id && (
            <Link to={`/projects/${e.project_id}`} className="label-mono text-text-quiet hover:text-text px-2 text-[11px]">
              Open project ↗
            </Link>
          )}
          <span className="flex-1" />
          <Button type="button" onClick={onClose}>Cancel</Button>
          <Button type="submit" variant="primary" disabled={busy || !projectPicked}>Save</Button>
        </div>
      </form>
    </Modal>
  );
}

function Avatar({ name, dashed = false }: { name: string; dashed?: boolean }) {
  return (
    <span
      className={`label-mono bg-surface-raised grid size-9 shrink-0 place-items-center rounded-full border-[1.5px] text-[10px] font-semibold ${dashed ? "border-dashed" : ""} border-line`}
      aria-hidden="true"
    >
      {initials(name)}
    </span>
  );
}

function People({ occ }: { occ: Occurrence }) {
  const me = useMe();
  return (
    <div className="flex flex-col">
      <span className="label-mono text-text-quiet mb-1">Who's working on it</span>
      <div className="border-rule flex min-h-12 items-center gap-3 border-x-0 border-t-0">
        <Avatar name={occ.event.user_name} />
        <span><b>{occ.event.user_id === me.id ? "You" : occ.event.user_name}</b> <span className="text-text-quiet">planned it</span></span>
      </div>
      {occ.joiners.map((j) => (
        <div key={j.user_id} className="flex min-h-12 items-center gap-3">
          <Avatar name={j.name} />
          <span><b>{j.user_id === me.id ? "You" : j.name}</b> <span className="text-text-quiet">{j.user_id === me.id ? "are joining" : "is joining"}</span></span>
        </div>
      ))}
      {occ.joiners.length === 0 && occ.event.user_id === me.id && (
        <p className="text-text-quiet pt-2 text-[13px]">Nobody else has said they're coming yet.</p>
      )}
    </div>
  );
}

/** Someone else's session: the details and I'll join. */
function SessionView({ occ, onClose }: { occ: Occurrence; onClose: () => void }) {
  const join = useJoin();
  const e = occ.event;
  const color = projectColor(e.project_color ?? "slot-1");
  return (
    <Modal title={eventLabel(e)} onClose={onClose}>
      <div className="flex flex-col gap-4">
        <div className="-mt-3 flex flex-col gap-1">
          {e.project_id && (
            <Link to={`/projects/${e.project_id}`} className="label-mono hover:underline" style={{ color }}>
              {e.project_title} ›
            </Link>
          )}
          <span className="font-mono text-[13px]">
            {whenText(occ)} · {hoursText(e.minutes)}
            {e.repeat_days ? ` · repeats ${repeatText(e.repeat_days)}` : ""}
          </span>
        </div>
        <People occ={occ} />
        <ErrorNote error={join.error} />
        {occ.can_join && (
          <Button
            variant={occ.joined ? "ghost" : "primary"}
            className="min-h-13 text-base"
            disabled={join.isPending}
            onClick={() => join.mutate({ id: e.id, occurrence: occ.occurrence, join: !occ.joined })}
          >
            {occ.joined ? "Can't make it after all" : "I'll join"}
          </Button>
        )}
        <p className="text-text-quiet text-[13px] leading-snug">
          {occ.joined
            ? `${e.user_name} sees you on the session and in the project's activity.`
            : `${e.user_name} sees you on the session and in the project's activity. ${e.repeat_days ? "This joins just this week." : ""}`}
        </p>
        <div className="flex justify-end">
          <Button onClick={onClose}>Close</Button>
        </div>
      </div>
    </Modal>
  );
}
