import { useState } from "react";
import { ErrorNote } from "../components/ui";
import { CellEditor, CellValue, type CellChange, type Names } from "./Cell";
import { ColumnMenu } from "./ColumnMenu";
import { KIND_ICON, money, total, visibleColumns, type Column, type DatabaseDetail, type Kind, type Row, type View } from "./format";
import { useCreateRow, useUpdateRow } from "./queries";

const WIDTH: Record<string, string> = {
  text: "min-w-48",
  number: "min-w-24",
  currency: "min-w-28",
  select: "min-w-32",
  multi_select: "min-w-44",
  date: "min-w-28",
  checkbox: "min-w-20",
  url: "min-w-40",
  person: "min-w-36",
  relation: "min-w-48",
};

export interface ViewProps {
  detail: DatabaseDetail;
  view: View | null;
  rows: Row[];
  names: Names;
  members: { user_id: string; name: string }[];
  /** Hide editing chrome (column menus, add column) for embeds. */
  compact?: boolean;
  onOpenRow: (row: Row) => void;
  /** Values a new row starts with, such as the filter it was added under. */
  newRowValues?: Record<string, unknown>;
}

/** Rows as a spreadsheet-like table. Click a cell to edit it. */
export function TableView({ detail, view, rows, names, members, compact, onOpenRow, newRowValues }: ViewProps) {
  const columns = visibleColumns(detail.columns, view);
  const canEdit = detail.can_edit;
  const update = useUpdateRow();
  const create = useCreateRow(detail.database.id);
  const [editing, setEditing] = useState<{ row: string; column: string } | null>(null);
  const [adding, setAdding] = useState("");
  const titleColumn = detail.columns.find((c) => c.is_title)!;
  const sums = columns.filter((c) => c.kind === "number" || c.kind === "currency");

  const save = (row: Row, change: CellChange) => update.mutate({ id: row.id, ...change });
  const addRow = () => {
    const name = adding.trim();
    if (!name) return;
    create.mutate(
      { values: { ...newRowValues, [titleColumn.id]: name }, relations: {} },
      { onSuccess: () => setAdding("") },
    );
  };

  return (
    <div className="flex flex-col">
      <div className="overflow-x-auto">
        <table className="w-full border-collapse text-sm">
          <thead>
            <tr className="border-rule border-x-0 border-t-0">
              {columns.map((c) => (
                <th key={c.id} scope="col" className={`${WIDTH[c.kind] ?? "min-w-32"} p-0 text-left align-bottom font-normal`}>
                  {canEdit && !compact ? (
                    <ColumnMenu column={c} detail={detail} view={view} />
                  ) : (
                    <HeaderLabel column={c} />
                  )}
                </th>
              ))}
              {canEdit && !compact && (
                <th scope="col" className="w-12 p-0 align-bottom">
                  <ColumnMenu detail={detail} view={view} />
                </th>
              )}
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.id} className="border-rule group border-x-0 border-t-0 last:border-b-0">
                {columns.map((c) => {
                  const isEditing = editing?.row === row.id && editing.column === c.id;
                  const numeric = c.kind === "number" || c.kind === "currency";
                  return (
                    <td
                      key={c.id}
                      className={`relative px-2 py-1.5 align-top ${numeric ? "text-right" : ""} ${canEdit && c.kind !== "checkbox" ? "cursor-text" : ""} ${isEditing ? "" : "hover:bg-surface-raised"}`}
                      onClick={() => {
                        if (!canEdit || isEditing) return;
                        if (c.kind === "checkbox") save(row, { values: { [c.id]: !(row.values as Record<string, unknown>)[c.id] } });
                        else setEditing({ row: row.id, column: c.id });
                      }}
                    >
                      {isEditing ? (
                        <CellEditor
                          column={c}
                          row={row}
                          projectId={detail.database.project_id}
                          members={members}
                          onSave={(change) => save(row, change)}
                          onClose={() => setEditing(null)}
                        />
                      ) : (
                        <span className={`flex min-h-8 items-center ${numeric ? "justify-end" : ""} ${c.is_title ? "font-bold" : ""}`}>
                          <CellValue column={c} row={row} names={names} />
                          {c.is_title && (
                            <button
                              type="button"
                              aria-label={`Open ${String((row.values as Record<string, unknown>)[c.id] ?? "row")}`}
                              title="Open"
                              className="label-mono text-text-quiet hover:text-text ml-auto shrink-0 cursor-pointer px-1.5 text-[11px] opacity-100 sm:opacity-0 sm:group-hover:opacity-100 focus:opacity-100"
                              onClick={(e) => {
                                e.stopPropagation();
                                onOpenRow(row);
                              }}
                            >
                              ↗
                            </button>
                          )}
                        </span>
                      )}
                    </td>
                  );
                })}
                {canEdit && !compact && <td />}
              </tr>
            ))}
            {rows.length === 0 && (
              <tr>
                <td colSpan={columns.length + 1} className="text-text-quiet px-2 py-4 text-sm">
                  {view?.config.filters.length ? "No rows match this view." : "No rows yet."}
                </td>
              </tr>
            )}
          </tbody>
          {sums.length > 0 && rows.length > 1 && (
            <tfoot>
              <tr className="border-rule border-x-0 border-b-0">
                {columns.map((c) => (
                  <td key={c.id} className="label-mono text-text-quiet px-2 py-2 text-right text-[11px]">
                    {c.kind === "currency"
                      ? money(total(c, rows), c.options.currency)
                      : c.kind === "number"
                        ? total(c, rows).toLocaleString("en-GB", { maximumFractionDigits: 2 })
                        : c.is_title
                          ? <span className="block text-left">{rows.length} rows</span>
                          : ""}
                  </td>
                ))}
                {canEdit && !compact && <td />}
              </tr>
            </tfoot>
          )}
        </table>
      </div>
      {canEdit && (
        <form
          className="border-rule flex items-center gap-2 border-x-0 border-b-0 px-2"
          onSubmit={(e) => {
            e.preventDefault();
            addRow();
          }}
        >
          <span className="label-mono text-text-quiet text-[12px]" aria-hidden="true">+</span>
          <input
            className="placeholder:text-text-quiet min-h-11 flex-1 bg-transparent text-sm outline-none"
            placeholder="New row"
            aria-label="New row"
            value={adding}
            maxLength={10_000}
            onChange={(e) => setAdding(e.target.value)}
          />
        </form>
      )}
      <ErrorNote error={update.error ?? create.error} />
    </div>
  );
}

export function HeaderLabel({ column }: { column: Column }) {
  return (
    <span
      className={`label-mono text-text-quiet flex min-h-10 items-center gap-1.5 px-2 text-[11px] ${column.kind === "number" || column.kind === "currency" ? "justify-end" : ""}`}
    >
      <span aria-hidden="true" className="text-[10px] normal-case opacity-70">{KIND_ICON[column.kind as Kind] ?? ""}</span>
      <span className="truncate">{column.name}</span>
    </span>
  );
}
