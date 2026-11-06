import { useMemo } from "react";
import { Board, type BoardColumn } from "../tasks/Board";
import { CellValue } from "./Cell";
import { cell, rowTitle, visibleColumns, type Row } from "./format";
import { useCreateRow, useUpdateRow } from "./queries";
import type { ViewProps } from "./TableView";

const NONE = "__none__";

/** Rows as cards in lanes, one per choice of the view's select column. */
export function BoardView({ detail, view, rows, names, onOpenRow }: ViewProps) {
  const update = useUpdateRow();
  const create = useCreateRow(detail.database.id);
  const group = detail.columns.find((c) => c.id === view?.config.group_by && c.kind === "select");
  const shown = visibleColumns(detail.columns, view).filter((c) => !c.is_title && c.id !== group?.id).slice(0, 4);
  const titleColumn = detail.columns.find((c) => c.is_title)!;
  const sorted = (view?.config.sorts.length ?? 0) > 0;

  const columns: BoardColumn<Row>[] = useMemo(() => {
    if (!group) return [];
    const lanes = [{ id: NONE, name: `No ${group.name.toLowerCase()}`, color: null as string | null | undefined }, ...group.options.choices];
    return lanes
      .map((lane) => {
        const mine = rows.filter((r) => (cell(r, group.id) ?? NONE) === lane.id);
        return {
          id: lane.id,
          tasks: mine,
          header: (
            <div className="flex items-center gap-2 px-3 pt-3 pb-2">
              <span
                className="size-2.5 shrink-0 rounded-full"
                style={{ background: lane.color ? `var(--tmp-${lane.color})` : "var(--tmp-rule)" }}
                aria-hidden="true"
              />
              <h3 className="font-display flex-1 truncate text-[15px] font-extrabold">{lane.name}</h3>
              <span className="label-mono text-text-quiet text-[11px]">{mine.length}</span>
            </div>
          ),
          footer: detail.can_edit && (
            <button
              type="button"
              className="label-mono text-text-quiet hover:text-text hover:bg-surface-card m-2 mt-0 min-h-10 cursor-pointer rounded-md px-2 text-left text-[11px]"
              onClick={() => {
                const name = prompt(`New row in ${lane.name}`);
                if (name?.trim())
                  create.mutate({
                    values: { [titleColumn.id]: name.trim(), ...(lane.id === NONE ? {} : { [group.id]: lane.id }) },
                    relations: {},
                  });
              }}
            >
              + Add
            </button>
          ),
        };
      })
      .filter((l) => l.id !== NONE || l.tasks.length > 0);
  }, [rows, group, detail.can_edit]);

  if (!group)
    return <p className="text-text-quiet p-4 text-sm">A board needs a select column to make its lanes. Add one, then pick it in this view's settings.</p>;

  return (
    <div className="flex flex-col pt-3 sm:min-h-0 sm:flex-1">
      <Board<Row>
        columns={columns}
        reorderable={!sorted}
        canDrag={() => detail.can_edit}
        onMove={(id, lane, afterId) =>
          update.mutate({
            id,
            values: { [group.id]: lane === NONE ? null : lane },
            ...(sorted ? {} : { after_id: afterId }),
          })
        }
        renderCard={(row, lifted) => (
          <button
            type="button"
            onClick={() => onOpenRow(row)}
            className={`border-rule bg-surface-card flex w-full cursor-pointer flex-col gap-1.5 rounded-md p-3 text-left ${lifted ? "shadow-card rotate-1" : ""}`}
          >
            <span className="font-bold">{rowTitle(detail.columns, row)}</span>
            {shown.map((c) =>
              cell(row, c.id) !== undefined || (row.relations[c.id]?.length ?? 0) > 0 ? (
                <span key={c.id} className="text-text-quiet flex items-center gap-2 text-xs">
                  <span className="label-mono shrink-0 text-[9.5px]">{c.name}</span>
                  <span className="text-text min-w-0 truncate"><CellValue column={c} row={row} names={names} /></span>
                </span>
              ) : null,
            )}
          </button>
        )}
      />
    </div>
  );
}
