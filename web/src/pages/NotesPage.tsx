import { useEffect, useMemo, useRef, useState } from "react";
import { Link, NavLink, Outlet, useMatch, useNavigate, useSearchParams } from "react-router";
import { useCanvas } from "../canvas/queries";
import { Button, ErrorNote } from "../components/ui";
import { ago, folderTree, snippetParts, type FolderNode, type NoteSummary } from "../notes/format";
import { useCreateNote, useNotes, useNoteTags } from "../notes/queries";
import { useQuickNote } from "../notes/QuickNote";
import { projectColor } from "../theme/resolve";

const sideLink = (active: boolean) =>
  `flex min-h-9 items-center gap-2 rounded-md px-2.5 text-[14px] ${active ? "bg-accent text-on-accent font-bold" : "text-text hover:bg-surface-raised"}`;

/** The folder value for notes others have shared with you. */
export const SHARED_FOLDER = "@shared";

/** Whether a note belongs in the chosen folder: yours by path, or the shared group. */
function inFolder(n: NoteSummary, folder: string | null): boolean {
  if (folder === SHARED_FOLDER) return !!n.owner_name;
  return !folder || (!n.owner_name && n.path.startsWith(`${folder}/`));
}

/** Notes: folders and tags on the left, a list or one note on the right. */
export function NotesLayout() {
  const noteOpen = !!useMatch("/notes/:id");
  return (
    <div className="flex flex-col md:min-h-0 md:flex-1 md:flex-row">
      <aside
        aria-label="Folders and tags"
        className={`border-rule flex-col gap-5 border-y-0 border-l-0 p-3 md:flex md:w-[250px] md:shrink-0 md:overflow-y-auto ${noteOpen ? "hidden" : "hidden md:flex"}`}
      >
        <Sidebar />
      </aside>
      <div className="flex min-w-0 flex-1 flex-col md:min-h-0 md:overflow-y-auto">
        <Outlet />
      </div>
    </div>
  );
}

function useFilterParams() {
  const [params, setParams] = useSearchParams();
  const set = (changes: Record<string, string | null>) =>
    setParams(
      (p) => {
        const next = new URLSearchParams(p);
        for (const [k, v] of Object.entries(changes)) {
          if (v) next.set(k, v);
          else next.delete(k);
        }
        return next;
      },
      { replace: true },
    );
  return { q: params.get("q") ?? "", tag: params.get("tag"), folder: params.get("folder"), set };
}

function Sidebar() {
  const all = useNotes({});
  const tags = useNoteTags();
  const projects = useCanvas(false).data?.projects ?? [];
  const { tag, folder } = useFilterParams();
  const navigate = useNavigate();
  const go = (changes: Record<string, string | null>) => {
    const p = new URLSearchParams();
    for (const [k, v] of Object.entries(changes)) if (v) p.set(k, v);
    navigate(`/notes${p.size ? `?${p}` : ""}`);
  };
  const tree = useMemo(() => folderTree((all.data ?? []).filter((n) => !n.owner_name).map((n) => n.path)), [all.data]);
  const sharedCount = (all.data ?? []).filter((n) => n.owner_name).length;
  const colorFor = (name: string) => projects.find((p) => p.title.toLowerCase() === name.toLowerCase())?.color;

  const renderFolder = (node: FolderNode, depth: number, project: boolean) => {
    const active = folder === node.path && !tag;
    const color = project ? colorFor(node.name) : undefined;
    return (
      <li key={node.path}>
        <button type="button" onClick={() => go({ folder: node.path })} className={`${sideLink(active)} w-full cursor-pointer text-left`} style={{ paddingLeft: 10 + depth * 14 }}>
          {color && <span className="size-2.5 shrink-0 rounded-[2px]" style={{ background: projectColor(color) }} aria-hidden="true" />}
          <span className="min-w-0 flex-1 truncate">{node.name}</span>
          <span className={`label-mono text-[10px] ${active ? "" : "text-text-quiet"}`}>{node.count}</span>
        </button>
        {node.children.length > 0 && (
          <ul>{node.children.map((c) => renderFolder(c, depth + 1, node.name === "Projects" && depth === 0))}</ul>
        )}
      </li>
    );
  };

  return (
    <>
      <nav aria-label="Folders">
        <ul>
          <li>
            <button type="button" onClick={() => go({})} className={`${sideLink(!folder && !tag)} w-full cursor-pointer text-left`}>
              <span className="flex-1">All notes</span>
              <span className={`label-mono text-[10px] ${!folder && !tag ? "" : "text-text-quiet"}`}>{all.data?.length ?? ""}</span>
            </button>
          </li>
          {tree.map((n) => renderFolder(n, 0, false))}
          {sharedCount > 0 && (
            <li>
              <button
                type="button"
                onClick={() => go({ folder: SHARED_FOLDER })}
                className={`${sideLink(folder === SHARED_FOLDER && !tag)} w-full cursor-pointer text-left`}
              >
                <span className="min-w-0 flex-1 truncate">Shared with me</span>
                <span className={`label-mono text-[10px] ${folder === SHARED_FOLDER && !tag ? "" : "text-text-quiet"}`}>{sharedCount}</span>
              </button>
            </li>
          )}
        </ul>
      </nav>
      {(tags.data?.length ?? 0) > 0 && (
        <section aria-label="Tags">
          <h2 className="label-mono text-text-quiet px-2.5 pb-1.5 text-[11px] font-normal">Tags</h2>
          <ul className="flex flex-wrap gap-1.5 px-1.5">
            {tags.data!.map((t) => (
              <li key={t.tag}>
                <button
                  type="button"
                  onClick={() => go({ tag: tag === t.tag ? null : t.tag })}
                  className={`label-mono border-rule cursor-pointer rounded-sm px-1.5 py-1 text-[10.5px] ${tag === t.tag ? "bg-accent text-on-accent border-accent" : "text-text-quiet hover:text-text"}`}
                >
                  #{t.tag} <span className="opacity-70">{t.count}</span>
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
    </>
  );
}

/** The list: every note, a folder, a tag, or search results. */
export function NotesIndex() {
  const { q, tag, folder, set } = useFilterParams();
  const [typed, setTyped] = useState(q);
  const search = useRef<HTMLInputElement>(null);
  const startQuick = useQuickNote((s) => s.start);
  const create = useCreateNote();
  const navigate = useNavigate();

  // Debounce typing into the URL, which drives the query.
  useEffect(() => {
    const t = setTimeout(() => typed !== q && set({ q: typed.trim() || null }), 200);
    return () => clearTimeout(t);
  }, [typed]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        search.current?.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const notes = useNotes({ q: q || undefined, tag: tag ?? undefined, limit: 500 });
  const shown = (notes.data ?? []).filter((n) => inFolder(n, folder));
  const shared = folder === SHARED_FOLDER;
  const heading = q ? `Search` : tag ? `#${tag}` : shared ? "Shared with me" : folder ? folder.split("/").pop()! : "Notes";

  const newNote = () =>
    create.mutate(
      { folder: shared ? undefined : (folder ?? undefined), tags: tag ? [tag] : [] },
      { onSuccess: (n) => navigate(`/notes/${n.note.id}?edit=1`) },
    );

  return (
    <div className="flex flex-col gap-4 px-4 py-4 sm:px-6 sm:py-5">
      <div className="flex flex-wrap items-end gap-3">
        <div className="min-w-0 flex-1">
          {folder && !shared && <p className="label-mono text-text-quiet truncate text-[11px]">{folder}</p>}
          <h1 className="font-display truncate text-[30px] leading-tight font-extrabold tracking-[-0.03em]">{heading}</h1>
        </div>
        <Button onClick={() => startQuick()} className="min-h-10 px-3 text-sm">Quick note</Button>
        <Button variant="primary" onClick={newNote} disabled={create.isPending} className="min-h-10 px-3 text-sm">
          + Note
        </Button>
      </div>
      <label className="border-rule bg-surface flex min-h-11 items-center gap-2.5 rounded-md px-3 font-mono text-[13px] focus-within:border-accent">
        <span className="text-accent" aria-hidden="true">›</span>
        <span className="sr-only">Search notes</span>
        <input
          ref={search}
          type="search"
          value={typed}
          onChange={(e) => setTyped(e.target.value)}
          placeholder="search notes"
          className="min-w-0 flex-1 bg-transparent outline-none"
        />
        <span className="label-mono text-text-quiet hidden text-[10px] sm:inline">Ctrl K</span>
      </label>
      <PhoneFilters />
      <ErrorNote error={notes.error ?? create.error} />
      {!notes.data ? (
        <p className="label-mono text-text-quiet">Loading…</p>
      ) : shown.length === 0 ? (
        <EmptyNotes searching={!!(q || tag || folder)} />
      ) : (
        <ul className="grid grid-cols-1 gap-3 lg:grid-cols-2 2xl:grid-cols-3" aria-label="Notes">
          {shown.map((n) => (
            <li key={n.id} className="min-w-0">
              <NoteCard note={n} />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** On phones the sidebar is hidden: folders and tags fold into two selects. */
function PhoneFilters() {
  const all = useNotes({});
  const tags = useNoteTags();
  const { tag, folder, set } = useFilterParams();
  const folders = useMemo(() => {
    const out: string[] = [];
    const walk = (nodes: FolderNode[]) => nodes.forEach((n) => (out.push(n.path), walk(n.children)));
    walk(folderTree((all.data ?? []).filter((n) => !n.owner_name).map((n) => n.path)));
    if ((all.data ?? []).some((n) => n.owner_name)) out.push(SHARED_FOLDER);
    return out;
  }, [all.data]);
  const select = "border-rule bg-surface min-h-10 min-w-0 flex-1 rounded-md px-2 font-mono text-[12px] outline-none";
  return (
    <div className="flex gap-2 md:hidden">
      <select aria-label="Folder" className={select} value={folder ?? ""} onChange={(e) => set({ folder: e.target.value || null })}>
        <option value="">All folders</option>
        {folders.map((f) => (
          <option key={f} value={f}>{f === SHARED_FOLDER ? "Shared with me" : f}</option>
        ))}
      </select>
      <select aria-label="Tag" className={select} value={tag ?? ""} onChange={(e) => set({ tag: e.target.value || null })}>
        <option value="">Any tag</option>
        {(tags.data ?? []).map((t) => (
          <option key={t.tag} value={t.tag}>#{t.tag}</option>
        ))}
      </select>
    </div>
  );
}

function EmptyNotes({ searching }: { searching: boolean }) {
  return (
    <div className="border-rule text-text-quiet rounded-lg border-dashed p-6 text-center">
      {searching ? (
        <p>Nothing matches. Try fewer words, or another folder.</p>
      ) : (
        <>
          <p className="font-display text-text text-lg font-extrabold">No notes yet</p>
          <p className="mt-1">
            Start one with <strong>+ Note</strong>, jot something with <strong>Quick note</strong> (Alt N anywhere), or bring
            an Obsidian vault or Notion export in from <Link to="/settings" className="text-accent underline">Settings</Link>.
          </p>
        </>
      )}
    </div>
  );
}

export function NoteCard({ note, compact }: { note: NoteSummary; compact?: boolean }) {
  return (
    <NavLink
      to={`/notes/${note.id}`}
      className="border-rule bg-surface-card hover:border-accent flex h-full min-w-0 flex-col gap-1.5 rounded-md px-3.5 py-3"
    >
      <span className="flex items-baseline gap-2">
        <span className="font-display min-w-0 flex-1 truncate text-[16px] font-extrabold tracking-[-0.01em]">{note.title}</span>
        <span className="label-mono text-text-quiet shrink-0 text-[10px]">{ago(note.updated_at)}</span>
      </span>
      {note.owner_name && (
        <span className="label-mono text-text-quiet text-[10px]">
          {note.owner_name}’s · {note.can_edit ? "you can edit" : "view only"}
        </span>
      )}
      {!compact && (
        <span className="text-text-quiet line-clamp-2 text-[13.5px]">
          {note.snippet
            ? snippetParts(note.snippet).map((p, i) =>
                p.hit ? <mark key={i} className="bg-accent text-on-accent rounded-[2px] px-0.5">{p.text}</mark> : <span key={i}>{p.text}</span>,
              )
            : note.excerpt || <em>Empty</em>}
        </span>
      )}
      {(note.projects.length > 0 || note.tags.length > 0) && (
        <span className="label-mono flex flex-wrap gap-x-2 gap-y-1 text-[10px]">
          {note.projects.map((p) => (
            <span key={p.id} style={{ color: projectColor(p.color) }}>{p.title}</span>
          ))}
          {note.tags.map((t) => (
            <span key={t} className="text-text-quiet">#{t}</span>
          ))}
        </span>
      )}
    </NavLink>
  );
}
