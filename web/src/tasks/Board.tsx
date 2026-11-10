import {
  closestCenter,
  closestCorners,
  DndContext,
  DragOverlay,
  KeyboardSensor,
  PointerSensor,
  TouchSensor,
  useDroppable,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragOverEvent,
  type DragStartEvent,
  type CollisionDetection,
} from "@dnd-kit/core";
import {
  arrayMove,
  horizontalListSortingStrategy,
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { createContext, useContext, useEffect, useMemo, useState, type HTMLAttributes, type ReactNode } from "react";
import type { Task } from "../api/client";
import { cardAbove, findColumn, moveCard } from "./format";

/** Anything with an id can be a card: tasks, or database rows. */
type Card = { id: string };

export interface BoardColumn<T extends Card = Task> {
  id: string;
  header: ReactNode;
  /** Shown under the cards, such as "+ Add". */
  footer?: ReactNode;
  tasks: T[];
}

interface BoardProps<T extends Card = Task> {
  columns: BoardColumn<T>[];
  renderCard: (task: T, dragging: boolean) => ReactNode;
  /** A card was dropped: its column and the card now above it (null at the top). */
  onMove: (taskId: string, columnId: string, afterId: string | null) => void;
  /** False when the order inside a column is fixed (the global board sorts by due date). */
  reorderable?: boolean;
  /** Shown after the last column, such as "+ Column". */
  trailing?: ReactNode;
  /** False for cards that can't move, such as tasks in a project you can only view. */
  canDrag?: (task: T) => boolean;
  /**
   * Columns can be dragged by the grip in their header (see `ColumnGrip`):
   * called with the column and the one now on its left (null when first).
   */
  onMoveColumn?: (columnId: string, afterId: string | null) => void;
  /** Something shown between columns, such as the new-column form: after this column, or first when null. */
  insert?: { afterId: string | null; node: ReactNode };
}

const COLUMN = "column:";
const isColumnId = (id: unknown) => String(id).startsWith(COLUMN);

/** Column drags only collide with columns, card drags only with cards and column bodies. */
const collisions: CollisionDetection = (args) => {
  const columnDrag = isColumnId(args.active.id);
  const droppableContainers = args.droppableContainers.filter((c) => isColumnId(c.id) === columnDrag);
  return columnDrag ? closestCenter({ ...args, droppableContainers }) : closestCorners({ ...args, droppableContainers });
};

type GripProps = { ref: (el: HTMLElement | null) => void; attributes: HTMLAttributes<HTMLElement>; listeners: object | undefined };
const GripContext = createContext<GripProps | null>(null);

/**
 * The handle that drags a column, for use inside a column header. Renders
 * nothing when the board can't reorder columns. Space or Enter picks the
 * column up for the keyboard, arrows move it, Space drops it.
 */
export function ColumnGrip({ label }: { label: string }) {
  const grip = useContext(GripContext);
  if (!grip) return null;
  return (
    <button
      type="button"
      ref={grip.ref}
      {...grip.attributes}
      {...grip.listeners}
      aria-label={label}
      className="text-text-quiet/70 hover:text-text -ml-1 grid h-8 w-5 shrink-0 cursor-grab touch-none place-items-center rounded-sm active:cursor-grabbing"
    >
      <svg width="10" height="16" viewBox="0 0 10 16" fill="currentColor" aria-hidden="true">
        <circle cx="2" cy="3" r="1.5" />
        <circle cx="8" cy="3" r="1.5" />
        <circle cx="2" cy="8" r="1.5" />
        <circle cx="8" cy="8" r="1.5" />
        <circle cx="2" cy="13" r="1.5" />
        <circle cx="8" cy="13" r="1.5" />
      </svg>
    </button>
  );
}

/**
 * A drag-and-drop kanban board. While dragging, the board keeps its own
 * order; after a drop it keeps that order until new columns arrive from the
 * cache, so a card never flicks back to where it came from.
 */
export function Board<T extends Card = Task>({
  columns,
  renderCard,
  onMove,
  reorderable = true,
  trailing,
  canDrag,
  onMoveColumn,
  insert,
}: BoardProps<T>) {
  // Keyed on the ids alone, so re-rendered headers don't count as new data.
  const signature = columns.map((c) => `${c.id}:${c.tasks.map((t) => t.id).join(",")}`).join("|");
  const fromProps = useMemo(
    () => Object.fromEntries(columns.map((c) => [c.id, c.tasks.map((t) => t.id)])),
    [signature],
  );
  const [order, setOrder] = useState<Record<string, string[]> | null>(null);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [startColumn, setStartColumn] = useState<string | null>(null);
  // Column order while one is dragged, and after the drop until the server agrees.
  const columnIds = columns.map((c) => c.id).join("|");
  const [columnOrder, setColumnOrder] = useState<string[] | null>(null);
  useEffect(() => setColumnOrder(null), [columnIds]);

  // New data from the server (or the cache) replaces any local order.
  useEffect(() => {
    if (!activeId) setOrder(null);
  }, [fromProps]);

  const byId = useMemo(() => new Map(columns.flatMap((c) => c.tasks.map((t) => [t.id, t] as const))), [columns]);
  const current = order ?? fromProps;

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(TouchSensor, { activationConstraint: { delay: 180, tolerance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  const onDragStart = ({ active }: DragStartEvent) => {
    if (isColumnId(active.id)) {
      setColumnOrder(columns.map((c) => c.id));
      return;
    }
    const id = String(active.id);
    setActiveId(id);
    setStartColumn(findColumn(fromProps, id) ?? null);
    setOrder(fromProps);
  };

  const onDragOver = ({ active, over }: DragOverEvent) => {
    if (!over) return;
    if (isColumnId(active.id)) {
      if (!isColumnId(over.id) || over.id === active.id) return;
      const from = String(active.id).slice(COLUMN.length);
      const to = String(over.id).slice(COLUMN.length);
      setColumnOrder((o) => {
        const base = o ?? columns.map((c) => c.id);
        return arrayMove(base, base.indexOf(from), base.indexOf(to));
      });
      return;
    }
    setOrder((o) => {
      const base = o ?? fromProps;
      const id = String(active.id);
      const from = findColumn(base, id);
      const to = findColumn(base, String(over.id));
      if (!from || !to || from === to) return base;
      const overIndex = base[to]!.indexOf(String(over.id));
      return moveCard(base, id, to, overIndex === -1 ? base[to]!.length : overIndex);
    });
  };

  const onDragEnd = ({ active, over }: DragEndEvent) => {
    if (isColumnId(active.id)) {
      const id = String(active.id).slice(COLUMN.length);
      const order = columnOrder ?? columns.map((c) => c.id);
      const was = columns.findIndex((c) => c.id === id);
      const now = order.indexOf(id);
      if (!over || was === now) {
        setColumnOrder(null);
        return;
      }
      onMoveColumn?.(id, now === 0 ? null : order[now - 1]!);
      return;
    }
    const id = String(active.id);
    setActiveId(null);
    const base = order ?? fromProps;
    const column = over ? findColumn(base, String(over.id)) : undefined;
    if (!over || !column) {
      setOrder(null);
      return;
    }
    let next = base;
    if (reorderable && over.id !== column) {
      const overIndex = base[column]!.indexOf(String(over.id));
      if (overIndex !== -1) next = moveCard(base, id, column, overIndex);
    }
    const moved = column !== startColumn || next[column]!.indexOf(id) !== fromProps[column]?.indexOf(id);
    if (!moved || (!reorderable && column === startColumn)) {
      setOrder(null);
      return;
    }
    setOrder(next);
    onMove(id, column, cardAbove(next, column, id));
  };

  const active = activeId ? byId.get(activeId) : undefined;
  const ordered = columnOrder
    ? columnOrder.map((id) => columns.find((c) => c.id === id)).filter((c): c is BoardColumn<T> => !!c)
    : columns;
  return (
    <DndContext
      sensors={sensors}
      collisionDetection={onMoveColumn ? collisions : closestCorners}
      onDragStart={onDragStart}
      onDragOver={onDragOver}
      onDragEnd={onDragEnd}
      onDragCancel={() => {
        setActiveId(null);
        setOrder(null);
        setColumnOrder(null);
      }}
    >
      <div className="flex min-w-0 gap-3 overflow-x-auto px-4 pb-4 sm:min-h-0 sm:flex-1 sm:px-5">
        <SortableContext items={ordered.map((c) => COLUMN + c.id)} strategy={horizontalListSortingStrategy}>
          {insert && insert.afterId === null && insert.node}
          {ordered.map((col) => (
            <ColumnSlot key={col.id} column={col} sortable={!!onMoveColumn}>
              <Column column={col} ids={current[col.id] ?? []} byId={byId} renderCard={renderCard} canDrag={canDrag} />
              {insert && insert.afterId === col.id && insert.node}
            </ColumnSlot>
          ))}
        </SortableContext>
        {trailing}
      </div>
      <DragOverlay dropAnimation={null}>{active ? renderCard(active, true) : null}</DragOverlay>
    </DndContext>
  );
}

/** Makes a column draggable by its grip, and hands the grip to the header through context. */
function ColumnSlot<T extends Card>({ column, sortable, children }: { column: BoardColumn<T>; sortable: boolean; children: ReactNode }) {
  const { attributes, listeners, setNodeRef, setActivatorNodeRef, transform, transition, isDragging } = useSortable({
    id: COLUMN + column.id,
    disabled: !sortable,
  });
  const grip = useMemo<GripProps | null>(
    () => (sortable ? { ref: setActivatorNodeRef, attributes, listeners } : null),
    [sortable, setActivatorNodeRef, attributes, listeners],
  );
  return (
    <GripContext.Provider value={grip}>
      <div
        ref={setNodeRef}
        data-dragging-column={isDragging || undefined}
        style={{ transform: CSS.Translate.toString(transform), transition }}
        className={`flex shrink-0 gap-3 ${isDragging ? "relative z-10 [&>section]:ring-accent [&>section]:shadow-card-selected [&>section]:-rotate-1 [&>section]:ring-2" : ""}`}
      >
        {children}
      </div>
    </GripContext.Provider>
  );
}

function Column<T extends Card>({
  column,
  ids,
  byId,
  renderCard,
  canDrag,
}: {
  column: BoardColumn<T>;
  ids: string[];
  byId: Map<string, T>;
  renderCard: BoardProps<T>["renderCard"];
  canDrag: BoardProps<T>["canDrag"];
}) {
  const { setNodeRef, isOver } = useDroppable({ id: column.id });
  return (
    <section
      aria-label={typeof column.header === "string" ? column.header : undefined}
      data-column={column.id}
      className={`bg-surface-raised flex w-[280px] shrink-0 flex-col rounded-lg ${isOver ? "ring-accent ring-2" : ""}`}
    >
      {column.header}
      <SortableContext id={column.id} items={ids} strategy={verticalListSortingStrategy}>
        <ol ref={setNodeRef} className="flex min-h-16 flex-1 flex-col gap-2 overflow-y-auto px-2 pb-2">
          {ids.map((id) => {
            const task = byId.get(id);
            return task ? (
              <SortableCard key={id} task={task} renderCard={renderCard} disabled={canDrag ? !canDrag(task) : false} />
            ) : null;
          })}
        </ol>
      </SortableContext>
      {column.footer}
    </section>
  );
}

function SortableCard<T extends Card>({ task, renderCard, disabled }: { task: T; renderCard: BoardProps<T>["renderCard"]; disabled: boolean }) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: task.id, disabled });
  return (
    <li
      ref={setNodeRef}
      style={{ transform: CSS.Translate.toString(transform), transition }}
      className={`touch-manipulation ${isDragging ? "opacity-40" : ""}`}
      {...attributes}
      {...listeners}
    >
      {renderCard(task, false)}
    </li>
  );
}
