import { useMemo, useState } from "react";
import { useMembers } from "../sharing/queries";
import { CellValue, Popover, type Names } from "./Cell";
import { display, money, rowTitle, total, type LinkedRows as Linked } from "./format";
import { useCreateRow, useDatabase, useRows, useUpdateRow } from "./queries";
import { RowDialog } from "./RowDialog";

/**
 * In a task's drawer: rows of the project's databases that link to the task,
 * with a total, and a way to link or add one.
 */
export function TaskLinkedRows({ groups, taskId, canEdit }: { groups: Linked[]; taskId: string; canEdit: boolean }) {
  return (
    <>
      {groups.map((g) => (
        <Group key={g.database.id} group={g} taskId={taskId} canEdit={canEdit} />
      ))}
    </>
  );
}

function Group({ group, taskId, canEdit }: { group: Linked; taskId: string; canEdit: boolean }) {
  const [open, setOpen] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const detail = useDatabase(open ? group.database.id : undefined);
  const members = useMembers(group.database.project_id).data ?? [];
  const names: Names = useMemo(() => Object.fromEntries(members.map((m) => [m.user_id, m.name])), [members]);
  const amount = group.columns.find((c) => c.kind === "currency") ?? group.columns.find((c) => c.kind === "number");
  const tag = group.columns.find((c) => c.kind === "select");
  const rows = group.rows;
  if (rows.length === 0 && !canEdit) return null;

  return (
    <section className="flex flex-col gap-1.5">
      <div className="flex items-baseline gap-2">
        <h3 className="label-mono text-text-quiet">Linked rows · {group.database.name}</h3>
        {amount && rows.length > 0 && (
          <span className="ml-auto font-mono text-[15px] font-semibold tabular-nums">
            {amount.kind === "currency" ? money(total(amount, rows), amount.options.currency) : total(amount, rows)}
          </span>
        )}
      </div>
      <div className="border-rule bg-surface relative rounded-md">
        <ul className="flex flex-col">
          {rows.map((r) => (
            <li key={r.id} className="border-rule border-x-0 border-t-0">
              <button
                type="button"
                onClick={() => setOpen(r.id)}
                className="hover:bg-surface-raised grid min-h-11 w-full cursor-pointer grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-2 px-3 text-left"
              >
                <span className="truncate">{rowTitle(group.columns, r)}</span>
                <span className="font-mono text-[13px] tabular-nums">{amount ? display(amount, r) : ""}</span>
                <span className="min-w-0">{tag && <CellValue column={tag} row={r} names={names} />}</span>
              </button>
            </li>
          ))}
        </ul>
        {canEdit && (
          <button
            type="button"
            className="label-mono text-text-quiet hover:text-text min-h-10 w-full cursor-pointer px-3 text-left text-[11px]"
            aria-expanded={adding}
            onClick={() => setAdding(true)}
          >
            + Link or add a row
          </button>
        )}
        {adding && <LinkPicker group={group} taskId={taskId} onClose={() => setAdding(false)} />}
      </div>
      {open && detail.data && (
        <RowDialog detail={detail.data} rowId={open} names={names} members={members} onClose={() => setOpen(null)} />
      )}
    </section>
  );
}

function LinkPicker({ group, taskId, onClose }: { group: Linked; taskId: string; onClose: () => void }) {
  const [q, setQ] = useState("");
  const rows = useRows(group.database.id, null, q);
  const create = useCreateRow(group.database.id);
  const update = useUpdateRow();
  const title = group.columns.find((c) => c.is_title)!;
  const linked = new Set(group.rows.map((r) => r.id));
  const link = { kind: "task", id: taskId };
  return (
    <Popover label={`Link a row of ${group.database.name}`} onClose={onClose}>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (q.trim())
            create.mutate(
              { values: { [title.id]: q.trim() }, relations: { [group.column_id]: [link] } },
              { onSuccess: onClose },
            );
        }}
      >
        <input
          autoFocus
          className="border-rule bg-surface mb-1 min-h-10 w-full rounded-sm px-2 font-mono text-[13px] outline-none focus:border-accent"
          placeholder="find a row, or name a new one"
          aria-label="Find or add a row"
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
      </form>
      {q.trim() && (
        <button
          type="button"
          className="hover:bg-surface-raised min-h-10 cursor-pointer rounded-sm px-2 text-left text-sm"
          onClick={() =>
            create.mutate({ values: { [title.id]: q.trim() }, relations: { [group.column_id]: [link] } }, { onSuccess: onClose })
          }
        >
          + Add “{q.trim()}”
        </button>
      )}
      {(rows.data ?? [])
        .filter((r) => !linked.has(r.id))
        .slice(0, 40)
        .map((r) => (
          <button
            key={r.id}
            type="button"
            className="hover:bg-surface-raised min-h-10 cursor-pointer truncate rounded-sm px-2 text-left text-sm"
            onClick={() =>
              update.mutate(
                { id: r.id, relations: { [group.column_id]: [...(r.relations[group.column_id] ?? []).map((x) => ({ kind: x.kind, id: x.id })), link] } },
                { onSuccess: onClose },
              )
            }
          >
            {rowTitle(group.columns, r)}
          </button>
        ))}
    </Popover>
  );
}
