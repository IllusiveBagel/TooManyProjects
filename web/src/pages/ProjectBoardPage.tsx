import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import type { Status } from "../api/client";
import { Button, ErrorNote } from "../components/ui";
import { Board, ColumnGrip, type BoardColumn } from "../tasks/Board";
import { NewColumnForm, RemoveColumn, fieldClass } from "../tasks/ColumnParts";
import { ColumnsPanel } from "../tasks/ColumnsPanel";
import { afterForStep, dismissWarning, doneWarning, isWarningDismissed, misnamed } from "../tasks/columns";
import { CATEGORIES, CATEGORY_COLOR, CATEGORY_LABEL } from "../tasks/format";
import { useMoveTask, useProjectTasks, useStatuses } from "../tasks/queries";
import { TaskCard } from "../tasks/TaskCard";
import { useOpenTask } from "../tasks/TaskDrawer";
import { useColumnActions } from "../tasks/useColumnActions";
import { useProject } from "./ProjectPage";

type Adding = { afterId: string | null } | null;

/** The Board tab: the project's own columns. Drag cards between them, and drag columns by their grip. */
export function ProjectBoardPage() {
  const { detail, newTask, canEdit, columnsOpen, setColumnsOpen } = useProject();
  const projectId = detail.project.id;
  const statuses = useStatuses(projectId);
  const tasks = useProjectTasks(projectId);
  const move = useMoveTask(projectId);
  const [, openTask] = useOpenTask();
  const [renaming, setRenaming] = useState<string | null>(null);
  const [removing, setRemoving] = useState<string | null>(null);
  const [menu, setMenu] = useState<string | null>(null);
  const [adding, setAdding] = useState<Adding>(null);
  const [, setDismissed] = useState(0);

  const list = useMemo(() => statuses.data ?? [], [statuses.data]);
  const counts = useMemo(() => {
    const c: Record<string, number> = {};
    for (const t of tasks.data ?? []) c[t.status_id] = (c[t.status_id] ?? 0) + 1;
    return c;
  }, [tasks.data]);
  const actions = useColumnActions(projectId, list, counts);
  const warning = canEdit ? doneWarning(list) : null;
  const showWarning = !!warning && !isWarningDismissed(projectId, list);

  const columns: BoardColumn[] = useMemo(() => {
    const all = tasks.data ?? [];
    return list.map((s) => {
      const mine = all.filter((t) => t.status_id === s.id);
      let header;
      if (renaming === s.id) header = <RenameColumn status={s} onDone={() => setRenaming(null)} rename={actions.rename} />;
      else if (removing === s.id)
        header = (
          <div className="p-3">
            <p className="font-display mb-2 text-[15px] font-extrabold">Remove {s.name}?</p>
            <RemoveColumn status={s} statuses={list} count={mine.length} onCancel={() => setRemoving(null)} onDone={() => setRemoving(null)} />
          </div>
        );
      else
        header = (
          <ColumnHeader
            status={s}
            statuses={list}
            count={mine.length}
            canEdit={canEdit}
            menuOpen={menu === s.id}
            onMenu={(open) => setMenu(open ? s.id : null)}
            onRename={() => setRenaming(s.id)}
            onRemove={() => setRemoving(s.id)}
            onAdd={(afterId) => setAdding({ afterId })}
            actions={actions}
          />
        );
      return {
        id: s.id,
        tasks: mine,
        header,
        footer: canEdit && (
          <button
            type="button"
            onClick={() => newTask(s.id)}
            className="label-mono text-text-quiet hover:text-text hover:bg-surface-card m-2 mt-0 min-h-10 cursor-pointer rounded-md px-2 text-left text-[11px]"
          >
            + Add
          </button>
        ),
      };
    });
  }, [list, tasks.data, renaming, removing, menu, newTask, canEdit, actions]);

  if (statuses.isError || tasks.isError) return <div className="p-5"><ErrorNote error={statuses.error ?? tasks.error} /></div>;
  if (!statuses.data || !tasks.data) return <p className="label-mono text-text-quiet p-5">Loading…</p>;
  const byId = new Map(list.map((s) => [s.id, s]));

  return (
    <div className="flex flex-col pt-3 sm:min-h-0 sm:flex-1">
      {showWarning && warning && (
        <div className="px-4 pb-3 sm:px-5">
          <div role="status" className="border-late bg-late/10 flex flex-wrap items-center gap-x-3 gap-y-2 rounded-lg border-[1.5px] px-3 py-2.5 sm:px-4">
            <span className="bg-late text-on-accent grid size-[22px] shrink-0 place-items-center rounded-sm font-mono text-[13px] font-extrabold" aria-hidden="true">
              !
            </span>
            <p className="min-w-0 flex-1 text-[13px] leading-snug sm:text-sm">
              <b>{warning.title}</b> <span className="text-text-quiet">{warning.detail}</span>
            </p>
            <div className="flex gap-2">
              <Button
                variant="primary"
                className="min-h-10 px-3 text-sm"
                onClick={() => (warning.fix ? actions.setCategory(warning.fix, "done") : setColumnsOpen(true))}
              >
                {warning.fix ? (
                  <>
                    <span className="sm:hidden">Fix</span>
                    <span className="hidden sm:inline">Make “{warning.fix.name}” count as done</span>
                  </>
                ) : (
                  "Pick a done column"
                )}
              </Button>
              <Button
                className="min-h-10 px-3 text-sm"
                onClick={() => {
                  dismissWarning(projectId, list);
                  setDismissed((n) => n + 1);
                }}
              >
                Not now
              </Button>
            </div>
          </div>
        </div>
      )}
      {canEdit && (
        <div className="flex items-center gap-2 px-4 pb-3 sm:hidden">
          <span className="label-mono text-text-quiet flex-1 text-[11px]">
            {list.length} columns · {tasks.data.length} tasks
          </span>
          <Button className="min-h-11 px-4 text-sm" onClick={() => setColumnsOpen(true)}>
            Columns
          </Button>
        </div>
      )}
      <Board
        columns={columns}
        onMove={(taskId, columnId, afterId) => move(taskId, byId.get(columnId)!, afterId)}
        renderCard={(t, lifted) => <TaskCard task={t} onOpen={openTask} lifted={lifted} />}
        onMoveColumn={canEdit ? (id, afterId) => actions.move(byId.get(id)!, afterId) : undefined}
        insert={
          adding
            ? {
                afterId: adding.afterId,
                node: (
                  <NewColumnForm
                    projectId={projectId}
                    statuses={list}
                    afterId={adding.afterId}
                    onDone={() => setAdding(null)}
                    className="h-fit w-[280px] shrink-0"
                  />
                ),
              }
            : undefined
        }
        trailing={
          canEdit &&
          !adding && (
            <div className="shrink-0">
              <button
                type="button"
                onClick={() => setAdding({ afterId: list.at(-1)?.id ?? null })}
                className="border-rule label-mono text-text-quiet hover:text-text min-h-11 w-36 shrink-0 cursor-pointer rounded-lg border-dashed text-[11px]"
              >
                + Column
              </button>
            </div>
          )
        }
        canDrag={() => canEdit}
      />
      {columnsOpen && (
        <ColumnsPanel
          projectId={projectId}
          projectTitle={detail.project.title}
          statuses={list}
          counts={counts}
          actions={actions}
          onClose={() => setColumnsOpen(false)}
        />
      )}
      {actions.ui}
    </div>
  );
}

function ColumnHeader({
  status,
  statuses,
  count,
  canEdit,
  menuOpen,
  onMenu,
  onRename,
  onRemove,
  onAdd,
  actions,
}: {
  status: Status;
  statuses: Status[];
  count: number;
  canEdit: boolean;
  menuOpen: boolean;
  onMenu: (open: boolean) => void;
  onRename: () => void;
  onRemove: () => void;
  onAdd: (afterId: string | null) => void;
  actions: ReturnType<typeof useColumnActions>;
}) {
  const wrong = misnamed(status);
  const left = afterForStep(statuses, status.id, -1);
  const right = afterForStep(statuses, status.id, 1);
  const before = statuses[statuses.findIndex((s) => s.id === status.id) - 1]?.id ?? null;

  // Alt + arrow keys move the column while its grip or menu button has focus.
  const onKeyDown = (e: KeyboardEvent) => {
    if (!canEdit || !e.altKey || (e.key !== "ArrowLeft" && e.key !== "ArrowRight")) return;
    const after = e.key === "ArrowLeft" ? left : right;
    e.preventDefault();
    if (after !== undefined) actions.move(status, after);
  };

  return (
    <div className="relative flex items-center gap-2 px-3 pt-3 pb-2" onKeyDown={onKeyDown}>
      {canEdit && <ColumnGrip label={`Move the ${status.name} column`} />}
      <span className="size-2.5 shrink-0 rounded-full" style={{ background: CATEGORY_COLOR[status.category] }} aria-hidden="true" />
      <div className="min-w-0 flex-1">
        <h2 className="font-display truncate text-[15px] font-extrabold">{status.name}</h2>
        {wrong && canEdit && (
          <p className="label-mono text-late text-[10px] leading-tight">Counts as {CATEGORY_LABEL[status.category]?.toLowerCase()}</p>
        )}
      </div>
      <span className="label-mono text-text-quiet text-[11px]">{count}</span>
      {canEdit && (
        <button
          type="button"
          aria-label={`${status.name} column options`}
          aria-haspopup="menu"
          aria-expanded={menuOpen}
          onClick={() => onMenu(!menuOpen)}
          className="text-text-quiet hover:text-text size-8 cursor-pointer rounded-sm text-lg leading-none"
        >
          ⋯
        </button>
      )}
      {menuOpen && (
        <ColumnMenu
          status={status}
          onClose={() => onMenu(false)}
          items={[
            { label: "Rename", hint: "", run: onRename },
            "categories",
            "-",
            { label: "← Move left", hint: "Alt ←", run: () => left !== undefined && actions.move(status, left), disabled: left === undefined },
            { label: "Move right →", hint: "Alt →", run: () => right !== undefined && actions.move(status, right), disabled: right === undefined },
            "-",
            { label: "+ Add column before", run: () => onAdd(before), disabled: statuses.length >= 20 },
            { label: "+ Add column after", run: () => onAdd(status.id), disabled: statuses.length >= 20 },
            "-",
            { label: "Remove column…", run: onRemove, danger: true, disabled: statuses.length <= 1 },
          ]}
          onCategory={(c) => actions.setCategory(status, c)}
        />
      )}
    </div>
  );
}

type MenuItem = { label: string; hint?: string; run: () => void; disabled?: boolean; danger?: boolean } | "-" | "categories";

/** The ⋯ menu on a column header. Arrow keys move between items; Escape or a click outside closes it. */
function ColumnMenu({
  status,
  items,
  onCategory,
  onClose,
}: {
  status: Status;
  items: MenuItem[];
  onCategory: (c: string) => void;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null;
    ref.current?.querySelector<HTMLElement>("[role^=menuitem]:not([disabled])")?.focus({ preventScroll: true });
    // A column near the edge of the board scrolls so its whole menu shows.
    ref.current?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node) && e.target !== opener) onClose();
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [onClose]);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const all = [...(ref.current?.querySelectorAll<HTMLElement>("[role^=menuitem]:not([disabled])") ?? [])];
    const i = all.indexOf(document.activeElement as HTMLElement);
    if (e.key === "Escape") {
      e.stopPropagation();
      onClose();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      all[(i + (e.key === "ArrowDown" ? 1 : all.length - 1)) % all.length]?.focus();
    }
  };
  const item = "flex min-h-9 w-full cursor-pointer items-center gap-2.5 rounded-sm px-2.5 text-left text-sm font-semibold hover:bg-line focus:bg-line focus:outline-none disabled:cursor-default disabled:opacity-35 disabled:hover:bg-transparent";

  return (
    <div
      ref={ref}
      role="menu"
      aria-label={`${status.name} column`}
      onKeyDown={onKeyDown}
      className="border-rule bg-surface-raised shadow-card absolute top-11 right-2 z-30 flex w-64 flex-col rounded-lg p-1.5"
    >
      {items.map((it, i) =>
        it === "-" ? (
          <div key={i} className="bg-line mx-1 my-1 h-[1.5px]" role="separator" />
        ) : it === "categories" ? (
          <div key={i} role="group" aria-label="Counts as" className="flex flex-col">
            <span className="label-mono text-text-quiet px-2.5 pt-1.5 pb-1 text-[10px]">Counts as</span>
            {CATEGORIES.map((c) => (
              <button
                key={c}
                type="button"
                role="menuitemradio"
                aria-checked={status.category === c}
                className={item}
                onClick={() => {
                  onCategory(c);
                  onClose();
                }}
              >
                <span className="size-2.5 shrink-0 rounded-full" style={{ background: CATEGORY_COLOR[c] }} aria-hidden="true" />
                {CATEGORY_LABEL[c]}
                {status.category === c && <span className="text-accent ml-auto">✓</span>}
              </button>
            ))}
          </div>
        ) : (
          <button
            key={i}
            type="button"
            role="menuitem"
            disabled={it.disabled}
            className={`${item} ${it.danger ? "text-late" : ""}`}
            onClick={() => {
              it.run();
              onClose();
            }}
          >
            {it.label}
            {it.hint && <span className="text-text-quiet ml-auto font-mono text-[11px] font-normal">{it.hint}</span>}
          </button>
        ),
      )}
    </div>
  );
}

function RenameColumn({ status, onDone, rename }: { status: Status; onDone: () => void; rename: (s: Status, name: string) => void }) {
  const [name, setName] = useState(status.name);
  return (
    <form
      className="flex gap-2 p-3"
      onSubmit={(e) => {
        e.preventDefault();
        e.currentTarget.querySelector("input")?.blur();
      }}
    >
      <input
        autoFocus
        aria-label="Column name"
        className={fieldClass}
        value={name}
        maxLength={40}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            setName(status.name);
            onDone();
          }
        }}
        onBlur={() => {
          rename(status, name);
          onDone();
        }}
      />
    </form>
  );
}
