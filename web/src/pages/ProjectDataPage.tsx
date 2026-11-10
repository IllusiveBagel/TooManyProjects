import { useEffect, useRef, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router";
import { Button, ErrorNote } from "../components/ui";
import { Popover } from "../data/Cell";
import { DatabaseView } from "../data/DatabaseView";
import { VIEW_LABEL, type DatabaseDetail, type View } from "../data/format";
import {
  useCreateDatabase,
  useCreateView,
  useDeleteDatabase,
  useDeleteView,
  useImportDatabase,
  useProjectDatabases,
  useUpdateDatabase,
  useUpdateView,
} from "../data/queries";
import { ViewControls } from "../data/ViewControls";
import { useProject } from "./ProjectPage";

const field = "border-rule bg-surface min-h-10 w-full rounded-sm px-2 font-mono text-[13px] outline-none focus:border-accent";
const tab = (on: boolean) =>
  `label-mono inline-flex min-h-9 shrink-0 cursor-pointer items-center gap-1.5 rounded-md px-2.5 text-[11px] ${on ? "bg-accent text-on-accent" : "text-text-quiet hover:text-text hover:bg-surface-raised"}`;

/** The Data tab: a project's databases, each with its saved views. */
export function ProjectDataPage() {
  const { detail, canEdit } = useProject();
  const projectId = detail.project.id;
  const { dbId } = useParams();
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const list = useProjectDatabases(projectId);

  const dbs = list.data ?? [];
  const current = dbs.find((d) => d.database.id === dbId) ?? dbs[0];
  useEffect(() => {
    if (list.data && current && current.database.id !== dbId) navigate(`/projects/${projectId}/data/${current.database.id}`, { replace: true });
  }, [list.data, current, dbId]);

  if (list.isError) return <div className="p-5"><ErrorNote error={list.error} /></div>;
  if (!list.data) return <p className="label-mono text-text-quiet p-5">Loading…</p>;

  const viewId = params.get("view");
  const view = current ? (current.views.find((v) => v.id === viewId) ?? current.views[0] ?? null) : null;
  const setView = (v: View) => setParams((p) => (p.set("view", v.id), p), { replace: true });

  return (
    <div className="flex flex-col gap-3 px-4 pt-4 pb-10 sm:min-h-0 sm:flex-1 sm:px-5">
      <nav aria-label="Databases" className="flex items-center gap-1.5 overflow-x-auto">
        {dbs.map((d) => (
          <Link key={d.database.id} to={`/projects/${projectId}/data/${d.database.id}`} className={tab(d.database.id === current?.database.id)}>
            {d.database.name}
            <span className="opacity-70">{d.database.row_count}</span>
          </Link>
        ))}
        {canEdit && <NewDatabase projectId={projectId} onMade={(id) => navigate(`/projects/${projectId}/data/${id}`)} startOpen={dbs.length === 0} />}
      </nav>

      {!current ? (
        <div className="border-rule max-w-xl rounded-lg border-dashed p-5">
          <h2 className="font-display text-lg font-extrabold">No databases yet</h2>
          <p className="text-text-quiet mt-1 text-sm leading-relaxed">
            A database is a table with typed columns: a parts list, an episode tracker, a reading list. Its views can show
            up in notes and on the overview, and rows can link to tasks.
            {canEdit ? " Make one above, or import a CSV (a Notion database export works)." : ""}
          </p>
        </div>
      ) : (
        <DatabasePanel key={current.database.id} detail={current} view={view} canEdit={canEdit} onView={setView} initialRow={params.get("row")} />
      )}
    </div>
  );
}

function DatabasePanel({
  detail,
  view,
  canEdit,
  onView,
  initialRow,
}: {
  detail: DatabaseDetail;
  view: View | null;
  canEdit: boolean;
  onView: (v: View) => void;
  initialRow?: string | null;
}) {
  const [q, setQ] = useState("");
  const updateView = useUpdateView();
  return (
    <section aria-label={detail.database.name} className="border-rule bg-surface-card shadow-card flex flex-col rounded-lg sm:min-h-0 sm:flex-1">
      <div className="border-rule flex flex-wrap items-center gap-2 border-x-0 border-t-0 px-3 py-2.5">
        <h2 className="font-display text-xl font-extrabold tracking-[-0.02em]">{detail.database.name}</h2>
        <span className="label-mono text-text-quiet text-[11px]">{detail.database.row_count} rows</span>
        <div className="ml-auto flex items-center gap-1.5">
          <a href={`/api/v1/databases/${detail.database.id}/export`} className="label-mono text-text-quiet hover:text-text px-2 text-[11px]" download>
            Export
          </a>
          {canEdit && <DatabaseMenu detail={detail} />}
        </div>
      </div>
      <div className="border-rule flex flex-wrap items-center gap-2 border-x-0 border-t-0 px-3 py-2">
        <nav aria-label="Views" className="flex w-full min-w-0 items-center gap-1 overflow-x-auto sm:w-auto">
          {detail.views.map((v) => (
            <button key={v.id} type="button" onClick={() => onView(v)} className={tab(v.id === view?.id)} aria-current={v.id === view?.id}>
              {v.name}
              {v.kind !== "table" && <span className="opacity-70">{VIEW_LABEL[v.kind]}</span>}
              {v.pinned && <span title="On the overview" aria-label="pinned">•</span>}
            </button>
          ))}
          {canEdit && <NewView detail={detail} onMade={onView} />}
        </nav>
        <div className="flex flex-1 flex-wrap items-center gap-1.5 sm:justify-end">
          <label className="border-rule flex min-h-9 w-full items-center gap-1.5 rounded-md px-2 sm:w-auto">
            <span className="text-accent font-mono text-[13px]" aria-hidden="true">›</span>
            <input
              className="min-w-0 flex-1 bg-transparent font-mono text-[12px] outline-none sm:w-40 sm:flex-none"
              placeholder="search rows"
              aria-label="Search rows"
              value={q}
              onChange={(e) => setQ(e.target.value)}
            />
          </label>
          {canEdit && view && <ViewControls detail={detail} view={view} />}
          {canEdit && view && (
            <button
              type="button"
              className={`label-mono border-rule hover:bg-surface-raised inline-flex min-h-9 cursor-pointer items-center rounded-md px-2.5 text-[11px] ${view.pinned ? "text-accent" : ""}`}
              aria-pressed={view.pinned}
              onClick={() => updateView.mutate({ id: view.id, pinned: !view.pinned })}
              title="Show this view on the project's overview"
            >
              {view.pinned ? "On overview" : "Pin to overview"}
            </button>
          )}
          {canEdit && view && detail.views.length > 1 && <ViewMenu view={view} />}
        </div>
      </div>
      <div className="flex flex-col sm:min-h-0 sm:flex-1 sm:overflow-y-auto">
        <DatabaseView detail={detail} view={view} q={q} initialRow={initialRow} />
      </div>
      <ErrorNote error={updateView.error} />
    </section>
  );
}

function NewDatabase({ projectId, onMade, startOpen }: { projectId: string; onMade: (id: string) => void; startOpen: boolean }) {
  const [open, setOpen] = useState(startOpen);
  const [name, setName] = useState("");
  const create = useCreateDatabase(projectId);
  const importCsv = useImportDatabase(projectId);
  const file = useRef<HTMLInputElement>(null);
  return (
    <div className="relative shrink-0">
      <button type="button" className={tab(false)} aria-expanded={open} onClick={() => setOpen((o) => !o)}>+ Database</button>
      {open && (
        <Popover label="New database" onClose={() => setOpen(false)}>
          <form
            className="flex flex-col gap-2 p-1.5"
            onSubmit={(e) => {
              e.preventDefault();
              if (name.trim()) create.mutate(name.trim(), { onSuccess: (d) => (setOpen(false), setName(""), onMade(d.database.id)) });
            }}
          >
            <input autoFocus className={field} placeholder="Parts list" aria-label="Database name" maxLength={60} value={name} onChange={(e) => setName(e.target.value)} />
            <Button type="submit" variant="primary" className="min-h-9 text-sm" disabled={!name.trim() || create.isPending}>Make it</Button>
            <span className="label-mono text-text-quiet pt-1 text-[10px]">Or from a CSV file</span>
            <input
              ref={file}
              type="file"
              accept=".csv,text/csv"
              className="hidden"
              onChange={(e) => {
                const f = e.target.files?.[0];
                if (!f) return;
                const base = name.trim() || f.name.replace(/\.csv$/i, "").replace(/ [0-9a-f]{32}$/i, "").replace(/_all$/i, "");
                importCsv.mutate({ file: f, name: base }, { onSuccess: (d) => (setOpen(false), onMade(d.database.id)) });
                e.target.value = "";
              }}
            />
            <Button type="button" className="min-h-9 text-sm" disabled={importCsv.isPending} onClick={() => file.current?.click()}>
              {importCsv.isPending ? "Importing…" : "Import CSV…"}
            </Button>
            <p className="text-text-quiet text-xs">The first column becomes the name. Column types are guessed from the values.</p>
            <ErrorNote error={create.error ?? importCsv.error} />
          </form>
        </Popover>
      )}
    </div>
  );
}

function NewView({ detail, onMade }: { detail: DatabaseDetail; onMade: (v: View) => void }) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [kind, setKind] = useState("table");
  const create = useCreateView(detail.database.id);
  return (
    <div className="relative shrink-0">
      <button type="button" className={tab(false)} aria-expanded={open} onClick={() => setOpen((o) => !o)}>+ View</button>
      {open && (
        <Popover label="New view" onClose={() => setOpen(false)}>
          <form
            className="flex flex-col gap-2 p-1.5"
            onSubmit={(e) => {
              e.preventDefault();
              create.mutate(
                { name: name.trim() || VIEW_LABEL[kind]!, kind, config: { filters: [], sorts: [], hidden: [] } },
                { onSuccess: (v) => (setOpen(false), setName(""), onMade(v)) },
              );
            }}
          >
            <input autoFocus className={field} placeholder="To order" aria-label="View name" maxLength={40} value={name} onChange={(e) => setName(e.target.value)} />
            <div className="grid grid-cols-2 gap-1">
              {Object.entries(VIEW_LABEL).map(([k, label]) => (
                <button key={k} type="button" aria-pressed={kind === k} className={`${tab(kind === k)} justify-center`} onClick={() => setKind(k)}>
                  {label}
                </button>
              ))}
            </div>
            {kind === "board" && !detail.columns.some((c) => c.kind === "select") && (
              <p className="text-text-quiet text-xs">A board groups rows by a select column. Add one first.</p>
            )}
            {kind === "gallery" && (
              <p className="text-text-quiet text-xs">Cards led by each row’s first photo. Add photos from a row.</p>
            )}
            {kind === "calendar" && !detail.columns.some((c) => c.kind === "date") && (
              <p className="text-text-quiet text-xs">A calendar places rows by a date column. Add one first.</p>
            )}
            <ErrorNote error={create.error} />
            <Button type="submit" variant="primary" className="min-h-9 text-sm">Add view</Button>
          </form>
        </Popover>
      )}
    </div>
  );
}

function ViewMenu({ view }: { view: View }) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState(view.name);
  const update = useUpdateView();
  const remove = useDeleteView();
  return (
    <div className="relative">
      <button type="button" aria-label={`${view.name} view settings`} aria-expanded={open} className="text-text-quiet hover:text-text size-9 cursor-pointer text-lg" onClick={() => setOpen((o) => !o)}>
        ⋯
      </button>
      {open && (
        <Popover label="View settings" align="right" onClose={() => setOpen(false)}>
          <form
            className="flex flex-col gap-2 p-1.5"
            onSubmit={(e) => {
              e.preventDefault();
              update.mutate({ id: view.id, name }, { onSuccess: () => setOpen(false) });
            }}
          >
            <input className={field} aria-label="View name" maxLength={40} value={name} onChange={(e) => setName(e.target.value)} />
            <Button type="submit" className="min-h-9 text-sm">Rename</Button>
            <Button type="button" className="text-late min-h-9 text-sm" onClick={() => confirm(`Delete the ${view.name} view? The rows stay.`) && remove.mutate(view.id)}>
              Delete view
            </Button>
            <ErrorNote error={update.error ?? remove.error} />
          </form>
        </Popover>
      )}
    </div>
  );
}

function DatabaseMenu({ detail }: { detail: DatabaseDetail }) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState(detail.database.name);
  const update = useUpdateDatabase();
  const remove = useDeleteDatabase();
  return (
    <div className="relative">
      <button type="button" aria-label="Database settings" aria-expanded={open} className="text-text-quiet hover:text-text size-9 cursor-pointer text-lg" onClick={() => setOpen((o) => !o)}>
        ⋯
      </button>
      {open && (
        <Popover label="Database settings" align="right" onClose={() => setOpen(false)}>
            <form
              className="flex flex-col gap-2 p-1.5"
              onSubmit={(e) => {
                e.preventDefault();
                update.mutate({ id: detail.database.id, name }, { onSuccess: () => setOpen(false) });
              }}
            >
              <input className={field} aria-label="Database name" maxLength={60} value={name} onChange={(e) => setName(e.target.value)} />
              <Button type="submit" className="min-h-9 text-sm">Rename</Button>
              <p className="text-text-quiet text-xs">
                Show a view in a note with <code className="font-mono">![[{detail.database.name}#View name]]</code>.
              </p>
              <Button
                type="button"
                className="text-late min-h-9 text-sm"
                onClick={() =>
                  confirm(`Delete ${detail.database.name} and its ${detail.database.row_count} rows? This can't be undone.`) &&
                  remove.mutate(detail.database.id)
                }
              >
                Delete database
              </Button>
              <ErrorNote error={update.error ?? remove.error} />
            </form>
        </Popover>
      )}
    </div>
  );
}
