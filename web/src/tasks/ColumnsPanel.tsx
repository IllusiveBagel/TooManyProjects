import {
  closestCenter,
  DndContext,
  KeyboardSensor,
  MouseSensor,
  TouchSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import { arrayMove, SortableContext, sortableKeyboardCoordinates, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { Fragment, useEffect, useId, useState } from "react";
import type { Status } from "../api/client";
import { Button, ErrorNote } from "../components/ui";
import { CategoryChoice, fieldBase, NewColumnForm, RemoveColumn } from "./ColumnParts";
import { afterForStep, misnamed } from "./columns";
import { CATEGORIES, CATEGORY_COLOR, CATEGORY_LABEL } from "./format";
import type { useColumnActions } from "./useColumnActions";

type Actions = ReturnType<typeof useColumnActions>;

const MAX_COLUMNS = 20;

/**
 * Every column of a board in one list: reorder by dragging or with the
 * arrows, rename, change what each counts as, add one anywhere, remove one.
 * A side panel on a computer, a sheet on a phone. Changes save as you go.
 */
export function ColumnsPanel({
  projectId,
  projectTitle,
  statuses,
  counts,
  actions,
  onClose,
}: {
  projectId: string;
  projectTitle: string;
  statuses: Status[];
  counts: Record<string, number>;
  actions: Actions;
  onClose: () => void;
}) {
  const [open, setOpen] = useState<string | null>(null);
  const [adding, setAdding] = useState<{ afterId: string | null; choose: boolean } | null>(null);
  const titleId = useId();

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement;
      if (e.key === "Escape" && !typing && !adding) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, adding]);

  const sensors = useSensors(
    useSensor(MouseSensor, { activationConstraint: { distance: 4 } }),
    useSensor(TouchSensor, { activationConstraint: { delay: 120, tolerance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  const onDragEnd = ({ active, over }: DragEndEvent) => {
    if (!over || active.id === over.id) return;
    const from = statuses.findIndex((s) => s.id === active.id);
    const to = statuses.findIndex((s) => s.id === over.id);
    // Dropped on a row: take its place, pushing it along.
    const next = arrayMove(statuses, from, to);
    actions.move(statuses[from]!, to === 0 ? null : next[to - 1]!.id);
  };

  const form = (afterId: string | null, choose: boolean) => (
    <NewColumnForm
      projectId={projectId}
      statuses={statuses}
      afterId={afterId}
      choosePlace={choose}
      onDone={() => setAdding(null)}
      className="my-2"
    />
  );
  const full = statuses.length >= MAX_COLUMNS;

  return (
    <>
      <div className="bg-shadow/60 fixed inset-0 z-40" onClick={onClose} aria-hidden="true" />
      <aside
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className="border-rule bg-surface-card fixed inset-x-0 top-12 bottom-0 z-50 flex flex-col overflow-hidden rounded-t-lg border-b-0 sm:inset-y-0 sm:top-0 sm:right-0 sm:left-auto sm:w-[520px] sm:rounded-none sm:border-y-0 sm:border-r-0"
      >
        <div className="bg-line mx-auto mt-2.5 h-1 w-9 rounded-full sm:hidden" aria-hidden="true" />
        <header className="border-rule flex items-center gap-3 border-x-0 border-t-0 px-4 py-3 sm:px-5 sm:py-4">
          <div className="min-w-0 flex-1">
            <p className="label-mono text-text-quiet truncate text-[11px]">{projectTitle}</p>
            <h2 id={titleId} className="font-display text-[24px] font-extrabold tracking-[-0.02em]">
              Columns
            </h2>
          </div>
          <Button className="min-h-11 px-4 text-sm" onClick={onClose}>
            Done
          </Button>
        </header>
        <div className="flex-1 overflow-y-auto px-4 py-3 sm:px-5">
          <p className="text-text-quiet mb-3 text-[13px] sm:hidden">
            Drag a handle, or use the arrows. Tap a column to rename it or change what it counts as.
          </p>
          <div className="label-mono text-text-quiet hidden pb-2 pl-11 text-[10px] sm:flex">
            <span className="flex-1">Name</span>
            <span className="w-[150px]">Counts as</span>
            <span className="w-[116px]">Tasks</span>
          </div>
          {adding?.afterId === null && !adding.choose && form(null, false)}
          <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={onDragEnd}>
            <SortableContext items={statuses.map((s) => s.id)} strategy={verticalListSortingStrategy}>
              <ol className="flex flex-col">
                {statuses.map((s, i) => (
                  <Fragment key={s.id}>
                    <Row
                      status={s}
                      statuses={statuses}
                      count={counts[s.id] ?? 0}
                      first={i === 0}
                      last={i === statuses.length - 1}
                      open={open === s.id}
                      onToggle={() => setOpen(open === s.id ? null : s.id)}
                      onAdd={(afterId) => {
                        setOpen(null);
                        setAdding({ afterId, choose: false });
                      }}
                      full={full}
                      actions={actions}
                    />
                    {adding?.afterId === s.id && !adding.choose ? (
                      form(s.id, false)
                    ) : i < statuses.length - 1 && !full ? (
                      <li className="group relative hidden h-6 items-center sm:flex">
                        <span className="bg-accent h-0.5 flex-1 opacity-0 group-focus-within:opacity-100 group-hover:opacity-100" />
                        <button
                          type="button"
                          onClick={() => setAdding({ afterId: s.id, choose: false })}
                          className="label-mono border-accent text-accent bg-surface-card mx-2 min-h-6 cursor-pointer rounded-sm border-[1.5px] px-2 text-[10px] opacity-0 group-focus-within:opacity-100 group-hover:opacity-100 focus:opacity-100"
                        >
                          + Add column here
                        </button>
                        <span className="bg-accent h-0.5 flex-1 opacity-0 group-focus-within:opacity-100 group-hover:opacity-100" />
                      </li>
                    ) : (
                      <li className="h-2 sm:h-6" aria-hidden="true" />
                    )}
                  </Fragment>
                ))}
              </ol>
            </SortableContext>
          </DndContext>
          {adding?.choose && form(adding.afterId, true)}
          <ErrorNote error={actions.error} />
        </div>
        <footer className="border-rule flex items-center gap-3 border-x-0 border-b-0 px-4 py-3 sm:px-5">
          <Button
            className="min-h-12 flex-1 sm:min-h-11 sm:flex-none"
            disabled={full}
            onClick={() => setAdding({ afterId: statuses.at(-1)?.id ?? null, choose: true })}
          >
            + Add column
          </Button>
          <span className="label-mono text-text-quiet hidden text-[10px] sm:ml-auto sm:inline">
            Saved as you go · {statuses.length} of {MAX_COLUMNS}
          </span>
        </footer>
      </aside>
    </>
  );
}

const arrow = (d: string) => (
  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
    <path d={d} />
  </svg>
);
const UP = arrow("M12 19V5M5 12l7-7 7 7");
const DOWN = arrow("M12 5v14M5 12l7 7 7-7");

function Row({
  status,
  statuses,
  count,
  first,
  last,
  open,
  onToggle,
  onAdd,
  full,
  actions,
}: {
  status: Status;
  statuses: Status[];
  count: number;
  first: boolean;
  last: boolean;
  open: boolean;
  onToggle: () => void;
  onAdd: (afterId: string | null) => void;
  full: boolean;
  actions: Actions;
}) {
  const { attributes, listeners, setNodeRef, setActivatorNodeRef, transform, transition, isDragging } = useSortable({
    id: status.id,
  });
  const [name, setName] = useState(status.name);
  const [removing, setRemoving] = useState(false);
  useEffect(() => setName(status.name), [status.name]);
  const wrong = misnamed(status);
  const step = (dir: -1 | 1) => {
    const after = afterForStep(statuses, status.id, dir);
    if (after !== undefined) actions.move(status, after);
  };
  const before = statuses[statuses.findIndex((s) => s.id === status.id) - 1]?.id ?? null;
  const commitName = () => (name.trim() ? actions.rename(status, name) : setName(status.name));

  const nameInput = (cls: string) => (
    <input
      aria-label={`Name of the ${status.name} column`}
      className={`${fieldBase} font-display text-[14px] font-bold ${cls}`}
      value={name}
      maxLength={40}
      onChange={(e) => setName(e.target.value)}
      onBlur={commitName}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
        if (e.key === "Escape") setName(status.name);
      }}
    />
  );
  const iconButton = "text-text grid size-11 shrink-0 cursor-pointer place-items-center rounded-md disabled:cursor-default disabled:opacity-25 sm:border-rule sm:size-9";

  return (
    <li
      ref={setNodeRef}
      style={{ transform: CSS.Translate.toString(transform), transition }}
      className={`border-rule bg-surface-raised relative rounded-md ${wrong ? "border-late" : ""} ${
        isDragging ? "border-accent shadow-card-selected z-10 -rotate-1" : ""
      }`}
    >
      <div className="flex min-h-[60px] items-center gap-2 px-1.5 sm:gap-2.5 sm:px-3">
        <button
          type="button"
          ref={setActivatorNodeRef}
          {...attributes}
          {...listeners}
          aria-label={`Drag ${status.name}`}
          className="text-text-quiet hover:text-text grid h-11 w-8 shrink-0 cursor-grab touch-none place-items-center rounded-sm active:cursor-grabbing"
        >
          <svg width="12" height="18" viewBox="0 0 10 16" fill="currentColor" aria-hidden="true">
            {[3, 8, 13].flatMap((y) => [<circle key={`a${y}`} cx="2" cy={y} r="1.5" />, <circle key={`b${y}`} cx="8" cy={y} r="1.5" />])}
          </svg>
        </button>
        {/* Phone: a summary that opens the column's details. */}
        <button type="button" onClick={onToggle} aria-expanded={open} className="flex min-w-0 flex-1 cursor-pointer items-center gap-2.5 text-left sm:hidden">
          <span className="size-2.5 shrink-0 rounded-full" style={{ background: CATEGORY_COLOR[status.category] }} aria-hidden="true" />
          <span className="flex min-w-0 flex-col gap-0.5">
            <span className="truncate text-[16px] font-extrabold">{status.name}</span>
            <span className={`label-mono text-[10px] ${wrong ? "text-late" : "text-text-quiet"}`}>
              {wrong ? "Counts as in progress" : CATEGORY_LABEL[status.category]} · {count} {count === 1 ? "task" : "tasks"}
            </span>
          </span>
        </button>
        {/* Computer: edit in the row. */}
        {nameInput("hidden min-h-9 min-w-0 flex-1 sm:block")}
        <select
          aria-label={`What ${status.name} counts as`}
          className={`${fieldBase} label-mono hidden min-h-9 w-[128px] shrink-0 px-2 text-[10px] sm:block`}
          value={status.category}
          onChange={(e) => actions.setCategory(status, e.target.value)}
        >
          {CATEGORIES.map((c) => (
            <option key={c} value={c}>
              {CATEGORY_LABEL[c]}
            </option>
          ))}
        </select>
        <span className="text-text-quiet hidden w-5 text-right font-mono text-[12px] sm:inline">{count}</span>
        <button type="button" className={iconButton} disabled={first} aria-label={`Move ${status.name} up`} onClick={() => step(-1)}>
          {UP}
        </button>
        <button type="button" className={iconButton} disabled={last} aria-label={`Move ${status.name} down`} onClick={() => step(1)}>
          {DOWN}
        </button>
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={open}
          aria-label={`More for ${status.name}`}
          className="text-text-quiet hover:text-text hidden size-9 shrink-0 cursor-pointer rounded-md text-lg sm:block"
        >
          ⋯
        </button>
      </div>
      {wrong && (
        <p className="label-mono text-late -mt-1 pb-2.5 pl-12 text-[10px] sm:pl-14">
          Called “{status.name}” but counts as {CATEGORY_LABEL[status.category]?.toLowerCase()} ·{" "}
          <button type="button" className="cursor-pointer underline" onClick={() => actions.setCategory(status, "done")}>
            Fix
          </button>
        </p>
      )}
      {open && (
        <div className="border-rule flex flex-col gap-3 border-x-0 border-b-0 p-3">
          <label className="flex flex-col gap-1.5 sm:hidden">
            <span className="label-mono text-text-quiet text-[10px]">Name</span>
            {nameInput("min-h-12 w-full text-[15px]")}
          </label>
          <div className="sm:hidden">
            <CategoryChoice big value={status.category} onChange={(c) => actions.setCategory(status, c)} />
            {status.category === "done" && count > 0 && (
              <p className="text-done mt-2 text-[13px]">Tasks here count as finished.</p>
            )}
          </div>
          {removing ? (
            <RemoveColumn status={status} statuses={statuses} count={count} onCancel={() => setRemoving(false)} onDone={onToggle} />
          ) : (
            <>
              <div className="grid grid-cols-2 gap-2">
                <Button className="min-h-12 text-sm sm:min-h-10" disabled={full} onClick={() => onAdd(before)}>
                  + Column before
                </Button>
                <Button className="min-h-12 text-sm sm:min-h-10" disabled={full} onClick={() => onAdd(status.id)}>
                  + Column after
                </Button>
              </div>
              <Button className="text-late min-h-12 border-current text-sm sm:min-h-10" onClick={() => setRemoving(true)}>
                Remove column…
              </Button>
            </>
          )}
        </div>
      )}
    </li>
  );
}
