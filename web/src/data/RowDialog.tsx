import { useState } from "react";
import { Modal } from "../components/Modal";
import { Button, ErrorNote } from "../components/ui";
import { AttachSection } from "../files/Files";
import { useAttachments } from "../files/queries";
import { CellEditor, CellValue, type CellChange, type Names } from "./Cell";
import { KIND_ICON, rowTitle, type DatabaseDetail, type Kind, type Row } from "./format";
import { useDeleteRow, useRows, useUpdateRow } from "./queries";

/** One row as a form: every column, click a value to change it. */
export function RowDialog({
  detail,
  rowId,
  names,
  members,
  onClose,
}: {
  detail: DatabaseDetail;
  rowId: string;
  names: Names;
  members: { user_id: string; name: string }[];
  onClose: () => void;
}) {
  // Read the row from the unfiltered list so it stays open after an edit moves it out of a view.
  const rows = useRows(detail.database.id);
  const row = rows.data?.find((r) => r.id === rowId);
  const update = useUpdateRow();
  const remove = useDeleteRow();
  const [editing, setEditing] = useState<string | null>(null);
  const [confirm, setConfirm] = useState(false);
  const files = useAttachments({ row_id: rowId });
  if (!row) return null;
  const canEdit = detail.can_edit;
  const save = (r: Row, change: CellChange) => update.mutate({ ...change, id: r.id });

  return (
    <Modal title={rowTitle(detail.columns, row)} onClose={onClose}>
      <dl className="flex flex-col">
        {detail.columns.map((c) => (
          <div key={c.id} className="border-rule grid grid-cols-[minmax(0,9rem)_1fr] items-start gap-3 border-x-0 border-t-0 py-1.5 last:border-b-0">
            <dt className="label-mono text-text-quiet flex min-h-9 items-center gap-1.5 text-[11px]">
              <span aria-hidden="true" className="text-[10px] normal-case opacity-70">{KIND_ICON[c.kind as Kind]}</span>
              <span className="truncate">{c.name}</span>
            </dt>
            <dd className="relative m-0 min-w-0">
              {editing === c.id ? (
                <CellEditor
                  column={c}
                  row={row}
                  projectId={detail.database.project_id}
                  members={members}
                  onSave={(change) => save(row, change)}
                  onClose={() => setEditing(null)}
                />
              ) : (
                <button
                  type="button"
                  disabled={!canEdit}
                  className="hover:bg-surface-raised flex min-h-9 w-full cursor-pointer items-center rounded-sm px-1.5 text-left disabled:cursor-default disabled:hover:bg-transparent"
                  onClick={() =>
                    c.kind === "checkbox"
                      ? save(row, { values: { [c.id]: !(row.values as Record<string, unknown>)[c.id] } })
                      : setEditing(c.id)
                  }
                >
                  <CellValue column={c} row={row} names={names} />
                  {canEdit && <span className="sr-only">Edit {c.name}</span>}
                </button>
              )}
            </dd>
          </div>
        ))}
      </dl>
      <div className="mt-4">
        <AttachSection
          target={{ row_id: row.id }}
          items={[...(files.data ?? [])].reverse()}
          name={rowTitle(detail.columns, row)}
          canEdit={canEdit}
          projectCanCover={canEdit}
        />
      </div>
      <ErrorNote error={update.error ?? remove.error} />
      <div className="mt-4 flex flex-wrap items-center gap-2">
        <span className="label-mono text-text-quiet text-[10px]">
          {detail.database.name} · added {new Date(row.created_at).toLocaleDateString("en-GB", { day: "numeric", month: "short" })}
        </span>
        {canEdit &&
          (confirm ? (
            <Button className="text-late ml-auto min-h-9 text-sm" onClick={() => remove.mutate(row.id, { onSuccess: onClose })}>
              Delete this row
            </Button>
          ) : (
            <Button className="text-late ml-auto min-h-9 text-sm" onClick={() => setConfirm(true)}>Delete…</Button>
          ))}
      </div>
    </Modal>
  );
}
