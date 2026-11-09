import { useEffect, useMemo, useRef, useState, useSyncExternalStore, type CSSProperties } from "react";
import { useSearchParams } from "react-router";
import { useMe } from "../auth/queries";
import { EventDialog, type Draft } from "../calendar/EventDialog";
import {
  DAY_NAMES,
  addDays,
  browserZone,
  clock,
  dayKey,
  eventLabel,
  hoursText,
  initials,
  isMine,
  live,
  localStart,
  minutesInto,
  placeDay,
  snap,
  summarise,
  weekLabel,
  weekStart,
  type Occurrence,
  type Placed,
} from "../calendar/format";
import { useCalendar, useUpdateEvent } from "../calendar/queries";
import { useCanvas } from "../canvas/queries";
import { Button, ErrorNote } from "../components/ui";
import { projectColor } from "../theme/resolve";

/** Height of one hour, in pixels. */
const HOUR = 40;
const PHONE = "(max-width: 639px)";

function usePhone(): boolean {
  return useSyncExternalStore(
    (cb) => {
      const m = window.matchMedia?.(PHONE);
      m?.addEventListener("change", cb);
      return () => m?.removeEventListener("change", cb);
    },
    () => window.matchMedia?.(PHONE).matches ?? false,
  );
}

function useNow(): Date {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), 60_000);
    return () => clearInterval(t);
  }, []);
  return now;
}

/** The week: plan sessions on projects, block out busy time, join friends' sessions. */
export function CalendarPage() {
  const me = useMe();
  const phone = usePhone();
  const now = useNow();
  const [params, setParams] = useSearchParams();
  const monday = useMemo(() => {
    const w = params.get("week");
    const parsed = w ? new Date(`${w}T00:00`) : now;
    return weekStart(Number.isNaN(parsed.getTime()) ? now : parsed);
    // The week only moves when the URL does, not every minute.
  }, [params.get("week")]);
  const mine = params.get("mine") === "1";
  const filter = useMemo(
    () => ({ from: monday.toISOString(), to: addDays(monday, 7).toISOString(), ...(mine ? { mine: true } : {}) }),
    [monday, mine],
  );
  const calendar = useCalendar(filter);
  const canvas = useCanvas(false);
  const list = calendar.data ?? [];
  const thisWeek = dayKey(weekStart(now)) === dayKey(monday);
  const [picked, setPicked] = useState<number>(() => (thisWeek ? (now.getDay() + 6) % 7 : 0));
  const [draft, setDraft] = useState<Draft | null>(null);
  const [open, setOpen] = useState<Occurrence | null>(null);

  // "Plan session" on a project's overview lands here with ?plan=<project>.
  useEffect(() => {
    const plan = params.get("plan");
    if (!plan) return;
    setDraft(defaultDraft(now, monday, plan === "new" ? undefined : plan));
    setParams(
      (p) => {
        const next = new URLSearchParams(p);
        next.delete("plan");
        return next;
      },
      { replace: true },
    );
  }, [params.get("plan")]);

  const summary = useMemo(() => summarise(monday, list, me.id), [monday, list, me.id]);
  const planned = new Set(summary.perProject.map((p) => p.id));
  const unplanned = (canvas.data?.projects ?? []).filter(
    (p) => p.status === "active" && p.role !== "viewer" && !planned.has(p.id),
  );

  const go = (weeks: number | null) =>
    setParams((p) => {
      const next = new URLSearchParams(p);
      if (weeks === null) next.delete("week");
      else next.set("week", dayKey(addDays(monday, weeks * 7)));
      return next;
    });
  const setMine = (on: boolean) =>
    setParams((p) => {
      const next = new URLSearchParams(p);
      if (on) next.set("mine", "1");
      else next.delete("mine");
      return next;
    });

  const days = phone ? [picked] : [0, 1, 2, 3, 4, 5, 6];
  const pickedDay = addDays(monday, picked);

  return (
    <div className="flex flex-col sm:min-h-0 sm:flex-1">
      <div className="flex flex-wrap items-end gap-x-6 gap-y-3 px-4 pt-4 pb-3 sm:px-5">
        <div className="min-w-0">
          <h1 className="font-display text-[26px] leading-tight font-extrabold tracking-[-0.03em] sm:text-[30px]">
            {phone
              ? pickedDay.toLocaleDateString("en-GB", { weekday: "long", day: "numeric" })
              : thisWeek
                ? "This week"
                : `Week of ${monday.toLocaleDateString("en-GB", { day: "numeric", month: "long" })}`}
          </h1>
          <p className="text-text-quiet hidden text-sm sm:block">
            {weekLabel(monday)}. Drag across empty time to plan. Busy time is only shown to you.
          </p>
        </div>
        <div className="ml-auto flex flex-wrap items-center gap-2">
          <div className="border-rule flex overflow-hidden rounded-md">
            <button type="button" aria-label="Previous week" onClick={() => go(-1)} className="label-mono hover:bg-surface-raised min-h-10 w-10 cursor-pointer">‹</button>
            <button type="button" onClick={() => go(null)} className="label-mono border-rule hover:bg-surface-raised min-h-10 cursor-pointer border-y-0 px-3 text-[11px]">Today</button>
            <button type="button" aria-label="Next week" onClick={() => go(1)} className="label-mono hover:bg-surface-raised min-h-10 w-10 cursor-pointer">›</button>
          </div>
          <div role="radiogroup" aria-label="Show" className="border-rule flex overflow-hidden rounded-md">
            {[
              [false, "Everyone"],
              [true, "Just mine"],
            ].map(([value, label]) => (
              <button
                key={String(value)}
                type="button"
                role="radio"
                aria-checked={mine === value}
                onClick={() => setMine(value as boolean)}
                className={`label-mono min-h-10 cursor-pointer px-3 text-[11px] ${mine === value ? "bg-accent text-on-accent font-semibold" : "text-text-quiet hover:text-text"}`}
              >
                {label as string}
              </button>
            ))}
          </div>
        </div>
      </div>

      {phone && (
        <div className="flex flex-col gap-2.5 px-4 pb-3">
          <div role="tablist" aria-label="Days" className="flex gap-1">
            {DAY_NAMES.map((name, i) => {
              const day = addDays(monday, i);
              const on = picked === i;
              const today = dayKey(day) === dayKey(now);
              const dots = list
                .filter((o) => o.event.kind === "session" && dayKey(new Date(o.start_at)) === dayKey(day))
                .slice(0, 3);
              return (
                <button
                  key={name}
                  type="button"
                  role="tab"
                  aria-selected={on}
                  onClick={() => setPicked(i)}
                  className={`border-rule flex min-h-14 flex-1 cursor-pointer flex-col items-center justify-center gap-0.5 rounded-md font-mono text-[10px] uppercase ${on ? "bg-accent text-on-accent border-accent" : today ? "text-accent border-accent" : "text-text-quiet"}`}
                >
                  {name}
                  <b className={`font-display text-[17px] normal-case ${on ? "" : "text-text"}`}>{day.getDate()}</b>
                  <span className="flex h-1.5 gap-0.5" aria-hidden="true">
                    {dots.map((o) => (
                      <span key={o.event.id + o.occurrence} className="size-1.5 rounded-full" style={{ background: on ? "currentColor" : projectColor(o.event.project_color ?? "slot-1") }} />
                    ))}
                  </span>
                </button>
              );
            })}
          </div>
          <div className="flex gap-1.5 overflow-x-auto">
            <span className="label-mono border-rule text-accent flex min-h-8 shrink-0 items-center rounded-md border-current px-2.5 text-[10.5px]">{hoursText(summary.planned)} planned</span>
            <span className="label-mono border-rule text-text-quiet flex min-h-8 shrink-0 items-center rounded-md px-2.5 text-[10.5px]">{hoursText(summary.free)} free</span>
            {summary.perProject.map((p) => (
              <span key={p.id} className="label-mono border-rule flex min-h-8 shrink-0 items-center gap-1.5 rounded-md px-2.5 text-[10.5px]" title={p.title}>
                <span className="size-2 rounded-sm" style={{ background: projectColor(p.color) }} aria-hidden="true" />
                {hoursText(p.minutes)}
              </span>
            ))}
          </div>
        </div>
      )}

      <div className="flex flex-col gap-5 px-4 pb-6 sm:min-h-0 sm:flex-1 sm:flex-row sm:items-start sm:px-5">
        <section aria-label="Week" className="border-rule bg-surface flex min-w-0 flex-1 flex-col overflow-hidden rounded-lg sm:max-h-full">
          {calendar.isError && <div className="p-4"><ErrorNote error={calendar.error} /></div>}
          <WeekGrid
            monday={monday}
            days={days}
            list={list}
            now={now}
            me={me.id}
            onDraft={setDraft}
            onOpen={setOpen}
          />
        </section>
        <aside className="flex flex-col gap-4 sm:w-[300px] sm:shrink-0">
          {!phone && (
            <section className="border-rule bg-surface-card shadow-card flex flex-col gap-1.5 rounded-lg p-4">
              <h2 className="font-display mb-1 text-xl font-extrabold tracking-[-0.02em]">This week</h2>
              <div className="border-rule flex gap-5 border-x-0 border-t-0 pb-3">
                <Stat value={hoursText(summary.planned)} label="Planned" />
                <Stat value={hoursText(summary.free)} label="Still free, 07–22" quiet />
              </div>
              {summary.perProject.length === 0 ? (
                <p className="text-text-quiet py-2 text-sm">Nothing planned yet. Drag on the week to plan a session.</p>
              ) : (
                summary.perProject.map((p) => (
                  <div key={p.id} className="flex items-center gap-2.5 py-1.5">
                    <span className="size-2.5 shrink-0 rounded-sm" style={{ background: projectColor(p.color) }} aria-hidden="true" />
                    <span className="w-28 shrink-0 truncate text-sm font-bold" title={p.title}>{p.title}</span>
                    <span
                      className="gauge flex-1"
                      style={{ "--gauge-value": (p.minutes / summary.perProject[0]!.minutes) * 100, "--gauge-color": projectColor(p.color) } as CSSProperties}
                      aria-hidden="true"
                    />
                    <span className="w-12 text-right font-mono text-[12px]">{hoursText(p.minutes)}</span>
                  </div>
                ))
              )}
            </section>
          )}
          {unplanned.length > 0 && (
            <section className="border-rule bg-surface-card flex flex-col rounded-lg p-4">
              <h2 className="font-display text-xl font-extrabold tracking-[-0.02em]">No time yet</h2>
              <p className="text-text-quiet mb-1 text-[13px]">Active projects with nothing planned this week.</p>
              {unplanned.slice(0, 8).map((p) => (
                <div key={p.id} className="border-rule flex min-h-12 items-center gap-2.5 border-x-0 border-t-0 last:border-b-0">
                  <span className="size-2.5 shrink-0 rounded-sm" style={{ background: projectColor(p.color) }} aria-hidden="true" />
                  <span className="min-w-0 flex-1 truncate font-bold">
                    {p.title}
                    {p.role !== "owner" && <span className="label-mono text-text-quiet ml-1.5 text-[9.5px]">{p.owner_name}'s</span>}
                  </span>
                  <Button className="label-mono min-h-9 px-2.5 text-[10.5px]" onClick={() => setDraft(defaultDraft(now, monday, p.id))}>
                    Plan
                  </Button>
                </div>
              ))}
            </section>
          )}
          <Key />
        </aside>
      </div>
      {(draft || open) && (
        <EventDialog
          draft={draft ?? undefined}
          occurrence={live(open, list) ?? undefined}
          onClose={() => {
            setDraft(null);
            setOpen(null);
          }}
        />
      )}
    </div>
  );
}

/** A two-hour session this evening, or on the viewed week's Monday. */
function defaultDraft(now: Date, monday: Date, projectId?: string): Draft {
  const inWeek = now >= monday && now < addDays(monday, 7);
  let day = inWeek ? new Date(now.getFullYear(), now.getMonth(), now.getDate()) : monday;
  if (inWeek && now.getHours() >= 19) day = addDays(day, 1);
  const start = new Date(day.getFullYear(), day.getMonth(), day.getDate(), 19, 0);
  return { start, end: new Date(start.getTime() + 2 * 3_600_000), kind: "session", projectId };
}

function Stat({ value, label, quiet = false }: { value: string; label: string; quiet?: boolean }) {
  return (
    <div className="flex flex-col">
      <span className={`font-mono text-[28px] font-semibold tracking-[-0.03em] ${quiet ? "text-text-quiet" : ""}`}>{value}</span>
      <span className="label-mono text-text-quiet text-[10px]">{label}</span>
    </div>
  );
}

function Key() {
  return (
    <section aria-label="Key" className="label-mono text-text-quiet hidden flex-col gap-2.5 px-1 text-[10px] sm:flex">
      <span className="flex items-center gap-2.5">
        <span className="bg-surface-card border-rule border-t-slot-1 box-border h-3.5 w-6 rounded-sm border-t-4" />Your session
      </span>
      <span className="flex items-center gap-2.5">
        <span className="border-slot-4 box-border h-3.5 w-6 rounded-sm border-[1.5px] border-t-4 border-dashed" />Someone else's · click to join
      </span>
      <span className="flex items-center gap-2.5">
        <span className="calendar-busy border-rule box-border h-3.5 w-6 rounded-sm" />Busy · only you see it
      </span>
    </section>
  );
}

type Drag =
  | { mode: "create"; day: number; from: number; to: number }
  | { mode: "move" | "resize"; occ: Occurrence; x: number; y: number; dMin: number; dDay: number; moved: boolean };

function WeekGrid({
  monday,
  days,
  list,
  now,
  me,
  onDraft,
  onOpen,
}: {
  monday: Date;
  days: number[];
  list: Occurrence[];
  now: Date;
  me: string;
  onDraft: (d: Draft) => void;
  onOpen: (o: Occurrence) => void;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  const columns = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  const update = useUpdateEvent();

  useEffect(() => {
    if (scroller.current) scroller.current.scrollTop = 7 * HOUR - 8;
  }, []);

  const placed = useMemo(
    () => days.map((i) => placeDay(addDays(monday, i), list)),
    [days, monday, list],
  );

  const minuteAt = (clientY: number) => {
    const rect = columns.current!.getBoundingClientRect();
    return Math.min(24 * 60, Math.max(0, ((clientY - rect.top) / HOUR) * 60));
  };
  const columnWidth = () => columns.current!.getBoundingClientRect().width / days.length;

  const startCreate = (e: React.PointerEvent, col: number) => {
    if (e.button !== 0 || e.target !== e.currentTarget) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    const m = Math.floor(minuteAt(e.clientY) / 15) * 15;
    setDrag({ mode: "create", day: days[col]!, from: m, to: m });
  };

  const startMove = (e: React.PointerEvent, occ: Occurrence, mode: "move" | "resize") => {
    if (e.button !== 0) return;
    e.stopPropagation();
    if (!occ.event.can_edit || occ.event.repeat_days !== 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    setDrag({ mode, occ, x: e.clientX, y: e.clientY, dMin: 0, dDay: 0, moved: false });
  };

  const onMove = (e: React.PointerEvent) => {
    if (!drag) return;
    if (drag.mode === "create") {
      setDrag({ ...drag, to: snap(minuteAt(e.clientY)) });
      return;
    }
    const dy = e.clientY - drag.y;
    const dx = e.clientX - drag.x;
    const moved = drag.moved || Math.abs(dy) > 4 || Math.abs(dx) > 4;
    const dMin = snap((dy / HOUR) * 60);
    const dDay = drag.mode === "move" && days.length > 1 ? Math.round(dx / columnWidth()) : 0;
    setDrag({ ...drag, moved, dMin, dDay });
  };

  const onUp = () => {
    if (!drag) return;
    setDrag(null);
    if (drag.mode === "create") {
      const day = addDays(monday, drag.day);
      let [a, b] = [Math.min(drag.from, drag.to), Math.max(drag.from, drag.to)];
      if (b - a < 15) b = Math.min(24 * 60, a + 60);
      const at = (m: number) => new Date(day.getFullYear(), day.getMonth(), day.getDate(), 0, m);
      onDraft({ start: at(a), end: at(b) });
      return;
    }
    if (!drag.moved) {
      onOpen(drag.occ);
      return;
    }
    const s = new Date(drag.occ.start_at);
    const minutes = drag.occ.event.minutes;
    if (drag.mode === "resize") {
      const next = Math.max(15, Math.min(1440, minutes + drag.dMin));
      update.mutate({ id: drag.occ.event.id, body: { minutes: next } });
    } else {
      const start = new Date(addDays(s, drag.dDay).getTime() + drag.dMin * 60_000);
      update.mutate({ id: drag.occ.event.id, body: { start_local: localStart(start), tz: browserZone() } });
    }
  };

  const nowMin = minutesInto(startOfDay(now), now);
  const todayIndex = Math.floor((now.getTime() - monday.getTime()) / 86_400_000);
  const cols = `52px repeat(${days.length}, minmax(0, 1fr))`;
  const dayCols = `repeat(${days.length}, minmax(0, 1fr))`;

  return (
    <>
      <div className={`border-rule grid border-x-0 border-t-0 ${days.length === 1 ? "hidden" : ""}`} style={{ gridTemplateColumns: cols }}>
        <span />
        {days.map((i) => {
          const day = addDays(monday, i);
          const today = dayKey(day) === dayKey(now);
          const mineMin = list
            .filter((o) => o.event.kind === "session" && isMine(o, me) && dayKey(new Date(o.start_at)) === dayKey(day))
            .reduce((n, o) => n + o.event.minutes, 0);
          return (
            <div
              key={i}
              className={`border-rule flex items-baseline gap-2 border-y-0 border-r-0 px-2.5 pt-2.5 pb-2 ${today ? "bg-surface-card shadow-[inset_0_-3px_0_var(--tmp-accent)]" : ""}`}
            >
              <span className={`label-mono ${today ? "text-accent" : "text-text-quiet"}`}>{DAY_NAMES[i]}</span>
              <b className="text-lg">{day.getDate()}</b>
              {mineMin > 0 && <span className="label-mono text-text-quiet ml-auto text-[10px]">{hoursText(mineMin)}</span>}
            </div>
          );
        })}
      </div>
      <div ref={scroller} className="max-h-[70dvh] overflow-y-auto sm:max-h-[calc(100dvh-230px)]">
        <div className="grid py-2" style={{ gridTemplateColumns: "52px minmax(0, 1fr)" }}>
          <div className="label-mono text-text-quiet relative text-[10px]" style={{ height: 24 * HOUR }} aria-hidden="true">
            {Array.from({ length: 23 }, (_, h) => (
              <span key={h} className="absolute right-2" style={{ top: (h + 1) * HOUR - 6 }}>
                {String(h + 1).padStart(2, "0")}
              </span>
            ))}
          </div>
          <div ref={columns} className="grid" style={{ gridTemplateColumns: dayCols }}>
            {days.map((i, col) => (
              <div
                key={i}
                data-testid={`day-${i}`}
                className={`calendar-hours border-rule relative touch-pan-y border-y-0 border-r-0 select-none ${drag?.mode === "create" ? "cursor-ns-resize" : "cursor-cell"}`}
                style={{ height: 24 * HOUR, "--hour": `${HOUR}px` } as CSSProperties}
                onPointerDown={(e) => startCreate(e, col)}
                onPointerMove={onMove}
                onPointerUp={onUp}
                onPointerCancel={() => setDrag(null)}
              >
                {placed[col]!.map((p) => (
                  <Block
                    key={p.occ.event.id + p.occ.occurrence}
                    p={p}
                    me={me}
                    drag={drag && drag.mode !== "create" && drag.occ === p.occ ? drag : null}
                    onPointerDown={startMove}
                    onPointerMove={onMove}
                    onPointerUp={onUp}
                    onOpen={onOpen}
                  />
                ))}
                {drag?.mode === "create" && drag.day === i && (
                  <div
                    className="border-accent bg-accent/10 pointer-events-none absolute inset-x-1 grid place-items-center rounded-md border-2 border-dashed"
                    style={{ top: (Math.min(drag.from, drag.to) / 60) * HOUR, height: Math.max(15, Math.abs(drag.to - drag.from)) / 60 * HOUR }}
                    aria-hidden="true"
                  >
                    <span className="label-mono text-accent text-[10px]">
                      {fmtMin(Math.min(drag.from, drag.to))}–{fmtMin(Math.max(drag.from, drag.to, Math.min(drag.from, drag.to) + 15))}
                    </span>
                  </div>
                )}
                {i === todayIndex && nowMin >= 0 && (
                  <div className="pointer-events-none absolute inset-x-0 z-10" style={{ top: (nowMin / 60) * HOUR }} aria-hidden="true">
                    <div className="bg-accent h-0.5" />
                    <span className="bg-accent absolute -top-1 -left-1 size-2.5 rounded-full" />
                  </div>
                )}
              </div>
            ))}
          </div>
        </div>
      </div>
    </>
  );
}

function startOfDay(now: Date): Date {
  return new Date(now.getFullYear(), now.getMonth(), now.getDate());
}

const fmtMin = (m: number) => `${String(Math.floor(m / 60) % 24).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;

function Block({
  p,
  me,
  drag,
  onPointerDown,
  onPointerMove,
  onPointerUp,
  onOpen,
}: {
  p: Placed;
  me: string;
  drag: Extract<Drag, { mode: "move" | "resize" }> | null;
  onPointerDown: (e: React.PointerEvent, o: Occurrence, mode: "move" | "resize") => void;
  onPointerMove: (e: React.PointerEvent) => void;
  onPointerUp: () => void;
  onOpen: (o: Occurrence) => void;
}) {
  const { occ } = p;
  const e = occ.event;
  const color = projectColor(e.project_color ?? "slot-1");
  const busy = e.kind === "busy";
  const mine = isMine(occ, me);
  let top = p.top;
  let height = p.bottom - p.top;
  let shiftX = 0;
  if (drag?.moved) {
    if (drag.mode === "move") {
      top += drag.dMin;
      shiftX = drag.dDay;
    } else height = Math.max(15, height + drag.dMin);
  }
  const short = height < 75;
  const movable = e.can_edit && e.repeat_days === 0;
  const look = busy
    ? "calendar-busy border-rule text-text-quiet"
    : mine
      ? "bg-surface-card border-rule border-t-4 shadow-button"
      : "border-[1.5px] border-dashed border-t-4 bg-surface/70";
  const s = new Date(occ.start_at);
  const end = new Date(occ.end_at);
  const label = `${busy ? "Busy" : e.project_title}: ${eventLabel(e)}, ${clock(s)} to ${clock(end)}${occ.joiners.length ? `, ${occ.joiners.map((j) => j.name).join(", ")} joining` : ""}`;
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      className={`absolute flex flex-col gap-0.5 overflow-hidden rounded-[4px] px-2 py-1 text-left text-[13px] leading-tight font-bold ${look} ${movable ? "cursor-grab active:cursor-grabbing" : "cursor-pointer"} ${drag?.moved ? "z-20 opacity-90" : "z-[1]"}`}
      style={{
        top: (top / 60) * HOUR,
        height: Math.max(18, (height / 60) * HOUR - 2),
        left: `calc(${(p.lane / p.lanes) * 100}% + 4px + ${shiftX * 100}%)`,
        width: `calc(${100 / p.lanes}% - 8px)`,
        ...(busy ? {} : { borderColor: mine ? undefined : color, borderTopColor: color }),
      }}
      onPointerDown={(ev) => (movable ? onPointerDown(ev, occ, "move") : ev.stopPropagation())}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onClick={() => {
        // Movable blocks open from pointer-up; this covers keyboard and the rest.
        if (!movable) onOpen(occ);
      }}
      onKeyDown={(ev) => {
        if (movable && (ev.key === "Enter" || ev.key === " ")) {
          ev.preventDefault();
          onOpen(occ);
        }
      }}
    >
      {!busy && !short && (
        <span className="label-mono truncate text-[9.5px]" style={{ color }}>
          {e.project_title}
          {e.user_id !== me ? ` · ${e.user_name}` : ""}
        </span>
      )}
      <span className={short ? "truncate" : "line-clamp-2"}>
        {eventLabel(e)}
        {short && <span className="text-text-quiet ml-1.5 font-mono text-[10.5px] font-normal">{clock(s)}</span>}
      </span>
      {!short && (
        <span className="text-text-quiet font-mono text-[10.5px] font-normal">
          {clock(s)}–{clock(end)}
          {e.repeat_days !== 0 && " ↻"}
        </span>
      )}
      {!busy && occ.joiners.length > 0 && height >= 90 && (
        <span className="mt-auto flex items-center gap-1">
          {[e.user_name, ...occ.joiners.map((j) => j.name)].slice(0, 4).map((n, i) => (
            <span
              key={n + i}
              className="label-mono bg-surface-raised border-text grid size-5 place-items-center rounded-full border-[1.5px] text-[8px] font-semibold"
              style={{ marginLeft: i ? -8 : 0 }}
              aria-hidden="true"
            >
              {initials(n)}
            </span>
          ))}
          <span className="text-accent ml-0.5 truncate font-mono text-[10px] font-normal">
            {occ.joiners.some((j) => j.user_id === me) ? "You're in" : `${occ.joiners[0]!.name.split(" ")[0]}${occ.joiners.length > 1 ? ` +${occ.joiners.length - 1}` : ""} coming`}
          </span>
        </span>
      )}
      {movable && !short && (
        <span
          className="absolute inset-x-0 bottom-0 h-2 cursor-ns-resize"
          onPointerDown={(ev) => onPointerDown(ev, occ, "resize")}
          aria-hidden="true"
        />
      )}
    </button>
  );
}
