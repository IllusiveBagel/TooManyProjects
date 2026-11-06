import {
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
} from "@dnd-kit/core";
import { SortableContext, sortableKeyboardCoordinates, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import type { Task } from "../api/client";
import { cardAbove, findColumn, moveCard } from "./format";

export interface BoardColumn {
  id: string;
  header: ReactNode;
  /** Shown under the cards, such as "+ Add". */
  footer?: ReactNode;
  tasks: Task[];
}

interface BoardProps {
  columns: BoardColumn[];
  renderCard: (task: Task, dragging: boolean) => ReactNode;
  /** A card was dropped: its column and the card now above it (null at the top). */
  onMove: (taskId: string, columnId: string, afterId: string | null) => void;
  /** False when the order inside a column is fixed (the global board sorts by due date). */
  reorderable?: boolean;
  /** Shown after the last column, such as "+ Column". */
  trailing?: ReactNode;
}

/**
 * A drag-and-drop kanban board. While dragging, the board keeps its own
 * order; after a drop it keeps that order until new columns arrive from the
 * cache, so a card never flicks back to where it came from.
 */
export function Board({ columns, renderCard, onMove, reorderable = true, trailing }: BoardProps) {
  // Keyed on the ids alone, so re-rendered headers don't count as new data.
  const signature = columns.map((c) => `${c.id}:${c.tasks.map((t) => t.id).join(",")}`).join("|");
  const fromProps = useMemo(
    () => Object.fromEntries(columns.map((c) => [c.id, c.tasks.map((t) => t.id)])),
    [signature],
  );
  const [order, setOrder] = useState<Record<string, string[]> | null>(null);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [startColumn, setStartColumn] = useState<string | null>(null);

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
    const id = String(active.id);
    setActiveId(id);
    setStartColumn(findColumn(fromProps, id) ?? null);
    setOrder(fromProps);
  };

  const onDragOver = ({ active, over }: DragOverEvent) => {
    if (!over) return;
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
  return (
    <DndContext
      sensors={sensors}
      collisionDetection={closestCorners}
      onDragStart={onDragStart}
      onDragOver={onDragOver}
      onDragEnd={onDragEnd}
      onDragCancel={() => {
        setActiveId(null);
        setOrder(null);
      }}
    >
      <div className="flex min-h-0 min-w-0 flex-1 gap-3 overflow-x-auto px-4 pb-4 sm:px-5">
        {columns.map((col) => (
          <Column key={col.id} column={col} ids={current[col.id] ?? []} byId={byId} renderCard={renderCard} />
        ))}
        {trailing}
      </div>
      <DragOverlay dropAnimation={null}>{active ? renderCard(active, true) : null}</DragOverlay>
    </DndContext>
  );
}

function Column({
  column,
  ids,
  byId,
  renderCard,
}: {
  column: BoardColumn;
  ids: string[];
  byId: Map<string, Task>;
  renderCard: BoardProps["renderCard"];
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
            return task ? <SortableCard key={id} task={task} renderCard={renderCard} /> : null;
          })}
        </ol>
      </SortableContext>
      {column.footer}
    </section>
  );
}

function SortableCard({ task, renderCard }: { task: Task; renderCard: BoardProps["renderCard"] }) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: task.id });
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
