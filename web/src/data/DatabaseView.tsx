import { useMemo, useState } from "react";
import { ErrorNote } from "../components/ui";
import { useMembers } from "../sharing/queries";
import { BoardView } from "./BoardView";
import { CalendarView } from "./CalendarView";
import { GalleryView } from "./GalleryView";
import type { Names } from "./Cell";
import type { DatabaseDetail, View } from "./format";
import { useRows } from "./queries";
import { RowDialog } from "./RowDialog";
import { TableView } from "./TableView";

/** A database through one of its views, with rows that open in a dialog. */
export function DatabaseView({
  detail,
  view,
  q,
  compact,
  limit,
  initialRow,
}: {
  detail: DatabaseDetail;
  view: View | null;
  q?: string;
  compact?: boolean;
  /** Show at most this many rows (embeds). */
  limit?: number;
  /** A row to open at once, such as one picked in search. */
  initialRow?: string | null;
}) {
  const rows = useRows(detail.database.id, view?.id, q);
  const members = useMembers(detail.database.project_id).data ?? [];
  const names: Names = useMemo(() => Object.fromEntries(members.map((m) => [m.user_id, m.name])), [members]);
  const [open, setOpen] = useState<string | null>(initialRow ?? null);

  // A new row added under an "is" filter starts with that value, so it stays in view.
  const newRowValues = useMemo(() => {
    const out: Record<string, unknown> = {};
    for (const f of view?.config.filters ?? []) {
      const col = detail.columns.find((c) => c.id === f.column_id);
      if (f.op === "is" && f.value !== undefined && f.value !== null && col && col.kind !== "relation") {
        out[f.column_id] = col.kind === "multi_select" ? [f.value] : f.value;
      }
    }
    return out;
  }, [view, detail.columns]);

  if (rows.isError) return <div className="p-3"><ErrorNote error={rows.error} /></div>;
  if (!rows.data) return <p className="label-mono text-text-quiet p-3 text-[11px]">Loading…</p>;
  const list = limit ? rows.data.slice(0, limit) : rows.data;
  const props = {
    detail,
    view,
    rows: list,
    names,
    members,
    compact,
    newRowValues,
    onOpenRow: (r: { id: string }) => setOpen(r.id),
  };
  const kind = view?.kind ?? "table";
  return (
    <>
      {kind === "board" ? (
        <BoardView {...props} />
      ) : kind === "calendar" ? (
        <CalendarView {...props} />
      ) : kind === "gallery" ? (
        <GalleryView {...props} />
      ) : (
        <TableView {...props} />
      )}
      {limit && rows.data.length > limit && (
        <p className="label-mono text-text-quiet px-3 py-2 text-[10.5px]">{rows.data.length - limit} more not shown</p>
      )}
      {open && <RowDialog detail={detail} rowId={open} names={names} members={members} onClose={() => setOpen(null)} />}
    </>
  );
}
