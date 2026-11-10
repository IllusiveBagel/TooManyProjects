import { fileUrl } from "../files/format";
import { CAMERA_ICON, Icon } from "../files/Files";
import { CellValue } from "./Cell";
import { rowTitle, visibleColumns } from "./format";
import { useCreateRow } from "./queries";
import type { ViewProps } from "./TableView";

/** Rows as cards, each led by its first photo. */
export function GalleryView({ detail, view, rows, names, compact, onOpenRow, newRowValues }: ViewProps) {
  const create = useCreateRow(detail.database.id);
  const titleColumn = detail.columns.find((c) => c.is_title)!;
  const shown = visibleColumns(detail.columns, view)
    .filter((c) => !c.is_title)
    .slice(0, compact ? 2 : 3);
  return (
    <div className="grid grid-cols-2 gap-3 p-3 sm:grid-cols-3 lg:grid-cols-4">
      {rows.map((r) => (
        <button
          key={r.id}
          type="button"
          onClick={() => onOpenRow(r)}
          className="border-rule bg-surface-card shadow-card group flex cursor-pointer flex-col overflow-hidden rounded-lg text-left"
        >
          <span className="bg-surface-raised text-text-quiet grid aspect-[4/3] w-full place-items-center overflow-hidden">
            {r.cover_id ? (
              <img src={fileUrl({ id: r.cover_id }, 400)} alt="" loading="lazy" className="size-full object-cover" />
            ) : (
              <Icon d={CAMERA_ICON} className="size-6 opacity-50" />
            )}
          </span>
          <span className="flex flex-col gap-1 p-2.5">
            <span className="font-display truncate text-[15px] font-extrabold">{rowTitle(detail.columns, r)}</span>
            {shown.map((c) => (
              <span key={c.id} className="flex min-h-5 items-center gap-1.5 text-[13px]">
                <span className="label-mono text-text-quiet w-16 shrink-0 truncate text-[10px]">{c.name}</span>
                <span className="min-w-0 truncate">
                  <CellValue column={c} row={r} names={names} />
                </span>
              </span>
            ))}
            {r.file_count > 1 && <span className="label-mono text-text-quiet text-[10px]">▣ {r.file_count}</span>}
          </span>
        </button>
      ))}
      {detail.can_edit && !compact && (
        <button
          type="button"
          className="border-rule label-mono text-text-quiet hover:text-text grid min-h-40 cursor-pointer place-items-center rounded-lg border-dashed text-[11px]"
          onClick={() => {
            const name = prompt("New row");
            if (name?.trim()) create.mutate({ values: { ...newRowValues, [titleColumn.id]: name.trim() }, relations: {} });
          }}
        >
          + Add
        </button>
      )}
    </div>
  );
}
