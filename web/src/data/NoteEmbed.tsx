import { Link } from "react-router";
import { DatabaseView } from "./DatabaseView";
import { resolveEmbed, VIEW_LABEL } from "./format";
import { useAllDatabases, useDatabase } from "./queries";

/**
 * A live database view inside a note, from `![[Parts list#To order]]`.
 * Cells can be edited here; the change goes to the database.
 */
export function NoteEmbed({ target, projectIds }: { target: string; projectIds: string[] }) {
  const all = useAllDatabases();
  const found = all.data ? resolveEmbed(target, all.data, projectIds) : null;
  const detail = useDatabase(found?.database.id);
  if (all.isPending) return <p className="label-mono text-text-quiet my-4 text-[11px]">Loading {target}…</p>;
  if (!found)
    return (
      <p className="label-mono border-rule text-text-quiet my-4 rounded-md border-dashed px-3 py-2 text-[11px] normal-case">
        No database called “{target.split("#")[0]}” that you can see.
      </p>
    );
  const view = detail.data?.views.find((v) => v.id === found.view?.id) ?? found.view;
  return (
    <section
      aria-label={`Embedded database view: ${found.database.name}${view ? `, ${view.name}` : ""}`}
      className="border-rule bg-surface-card shadow-card not-prose my-6 overflow-hidden rounded-lg font-sans text-[14px] leading-normal"
    >
      <header className="border-rule bg-surface-raised flex flex-wrap items-center gap-2.5 border-x-0 border-t-0 px-3 py-2">
        <span className="font-display text-base font-extrabold">{found.database.name}</span>
        {view && <span className="label-mono bg-accent text-on-accent rounded-sm px-1.5 py-0.5 text-[10.5px] font-semibold">{view.name}</span>}
        <span className="label-mono text-text-quiet text-[10.5px]">
          {VIEW_LABEL[view?.kind ?? "table"]} · {found.database.project_title}
        </span>
        <Link
          to={`/projects/${found.database.project_id}/data/${found.database.id}${view ? `?view=${view.id}` : ""}`}
          className="label-mono border-rule hover:bg-surface ml-auto inline-flex min-h-8 items-center rounded-sm px-2 text-[10.5px]"
        >
          Open ↗
        </Link>
      </header>
      {detail.data ? (
        <DatabaseView detail={detail.data} view={view} compact limit={50} />
      ) : (
        <p className="label-mono text-text-quiet p-3 text-[11px]">Loading…</p>
      )}
    </section>
  );
}
