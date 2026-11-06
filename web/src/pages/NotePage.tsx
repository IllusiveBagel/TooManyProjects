import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router";
import { useCanvas } from "../canvas/queries";
import { Button, ErrorNote } from "../components/ui";
import { MarkdownEditor } from "../notes/Editor";
import { ago, parseTags, type NoteDetail } from "../notes/format";
import { useAllDatabases } from "../data/queries";
import { NoteMarkdown } from "../notes/Markdown";
import { useCreateNote, useDeleteNote, useNote, useNotes, useUpdateNote } from "../notes/queries";
import { projectColor } from "../theme/resolve";
import { NoteCard } from "./NotesPage";

type SaveState = "saved" | "pending" | "saving" | "error";

/** One note: title, projects, tags, the text, and what links to it. */
export function NotePage() {
  const { id = "" } = useParams();
  const note = useNote(id);
  if (note.isError) return <div className="p-5"><ErrorNote error={note.error} /></div>;
  if (!note.data) return <p className="label-mono text-text-quiet p-5">Loading…</p>;
  // Keyed by id: the editor holds its own text, so another note needs a fresh one.
  return <NoteView key={id} detail={note.data} />;
}

function NoteView({ detail }: { detail: NoteDetail }) {
  const { note } = detail;
  const canEdit = note.can_edit;
  const [params, setParams] = useSearchParams();
  const editing = canEdit && (params.get("edit") === "1" || (!detail.body.trim() && params.get("edit") !== "0"));
  const setEditing = (on: boolean) =>
    setParams((p) => {
      const next = new URLSearchParams(p);
      next.set("edit", on ? "1" : "0");
      return next;
    }, { replace: true });

  const update = useUpdateNote(note.id);
  const remove = useDeleteNote();
  const create = useCreateNote();
  const navigate = useNavigate();
  const allNotes = useNotes({});
  const projects = useCanvas(false).data?.projects ?? [];

  // Autosave: the newest text waits here until a quiet moment.
  const [body, setBody] = useState(detail.body);
  const [save, setSave] = useState<SaveState>("saved");
  const pending = useRef<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const flush = useCallback(() => {
    clearTimeout(timer.current);
    const text = pending.current;
    if (text === null) return;
    pending.current = null;
    setSave("saving");
    update.mutate(
      { body: text },
      {
        onSuccess: () => setSave(pending.current === null ? "saved" : "pending"),
        onError: () => {
          pending.current ??= text;
          setSave("error");
        },
      },
    );
  }, [update]);
  const onChange = (text: string) => {
    setBody(text);
    pending.current = text;
    setSave("pending");
    clearTimeout(timer.current);
    timer.current = setTimeout(flush, 800);
  };
  const flushRef = useRef(flush);
  flushRef.current = flush;
  useEffect(() => {
    const warn = (e: BeforeUnloadEvent) => {
      if (pending.current !== null) e.preventDefault();
    };
    window.addEventListener("beforeunload", warn);
    return () => {
      window.removeEventListener("beforeunload", warn);
      flushRef.current(); // leaving the note: save what's left
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (canEdit && (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "e") {
        e.preventDefault();
        flushRef.current();
        setEditing(!editing);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const databases = useAllDatabases(editing);
  const targets = useMemo(
    () => ({
      notes: (allNotes.data ?? []).filter((n) => n.id !== note.id).map((n) => n.title),
      projects: projects.map((p) => p.title),
      views: (databases.data ?? []).flatMap((d) => d.views.map((v) => `${d.name}#${v.name}`)),
    }),
    [allNotes.data, projects, note.id, databases.data],
  );

  const createLinked = useCallback(
    (title: string) => create.mutate({ title }, { onSuccess: (n) => navigate(`/notes/${n.note.id}?edit=1`) }),
    [create, navigate],
  );

  const linksOut = detail.links;
  const saveLabel = { saved: `Saved · ${ago(note.updated_at)}`, pending: "Editing…", saving: "Saving…", error: "Couldn't save. Retrying when you type." }[save];

  return (
    <div className="flex flex-col lg:min-h-0 lg:flex-1 lg:flex-row">
      <article className="flex min-w-0 flex-1 flex-col gap-4 px-4 pt-4 pb-10 sm:px-8 sm:pt-6 lg:overflow-y-auto lg:px-12">
        <div className="flex flex-wrap items-center gap-2">
          <Link to="/notes" className="label-mono text-text-quiet hover:text-text md:hidden">‹ Notes</Link>
          <span className={`label-mono text-[11px] ${save === "error" ? "text-late" : "text-text-quiet"}`} role="status">{saveLabel}</span>
          {note.owner_name && (
            <span className="label-mono text-text-quiet text-[11px]">
              · {note.owner_name}’s note · {canEdit ? "you can edit" : "view only"}
            </span>
          )}
          <span className="ml-auto flex gap-2">
            {canEdit && (
              <Button
                className="label-mono min-h-9 px-3 text-[11px]"
                aria-pressed={editing}
                onClick={() => {
                  flush();
                  setEditing(!editing);
                }}
                title="Ctrl E"
              >
                {editing ? "Read" : "Edit"}
              </Button>
            )}
            <a
              href={`/api/v1/notes/${note.id}/download`}
              className="border-rule label-mono hover:bg-surface-raised inline-flex min-h-9 items-center rounded-md px-3 text-[11px] font-bold"
            >
              .md
            </a>
            {!note.owner_name && (
              <Button
                className="label-mono text-late min-h-9 px-3 text-[11px]"
                onClick={() => {
                  if (!confirm(`Delete “${note.title}”? The file is removed from your vault.`)) return;
                  pending.current = null;
                  remove.mutate(note.id, { onSuccess: () => navigate("/notes") });
                }}
              >
                Delete
              </Button>
            )}
          </span>
        </div>

        <TitleInput detail={detail} readOnly={!canEdit} onRename={(title) => update.mutate({ title })} />
        <ErrorNote error={update.error ?? remove.error ?? create.error} />

        <MetaRow detail={detail} readOnly={!canEdit} onChange={(change) => update.mutate(change)} />

        {editing ? (
          <MarkdownEditor value={body} onChange={onChange} targets={targets} autoFocus label={`${note.title}, markdown`} />
        ) : body.trim() ? (
          <div onDoubleClick={() => canEdit && setEditing(true)}>
            <NoteMarkdown body={body} links={linksOut} onCreate={createLinked} projectIds={note.projects.map((p) => p.id)} />
          </div>
        ) : canEdit ? (
          <button type="button" onClick={() => setEditing(true)} className="text-text-quiet font-reading cursor-pointer self-start text-[19px] italic">
            Empty. Start writing…
          </button>
        ) : (
          <p className="text-text-quiet font-reading text-[19px] italic">Empty.</p>
        )}
      </article>

      <aside
        aria-label="Links"
        className="border-rule flex flex-col gap-6 border-x-0 border-b-0 px-4 py-6 sm:px-8 lg:w-[290px] lg:shrink-0 lg:overflow-y-auto lg:border-t-0 lg:border-l lg:px-4"
      >
        <section className="flex flex-col gap-2">
          <h2 className="label-mono text-text-quiet text-[11px] font-normal">
            Backlinks <span className="text-accent">{detail.backlinks.length}</span>
          </h2>
          {detail.backlinks.length === 0 ? (
            <p className="text-text-quiet text-[13px]">Nothing links here yet. Link it with [[{note.title}]].</p>
          ) : (
            <ul className="flex flex-col gap-2">
              {detail.backlinks.map((n) => (
                <li key={n.id}><NoteCard note={n} compact /></li>
              ))}
            </ul>
          )}
        </section>
        <section className="flex flex-col gap-1.5">
          <h2 className="label-mono text-text-quiet text-[11px] font-normal">
            Links out <span className="text-accent">{linksOut.length}</span>
          </h2>
          {linksOut.map((l) =>
            l.id ? (
              <Link
                key={`${l.kind}:${l.title}`}
                to={l.kind === "project" ? `/projects/${l.id}` : `/notes/${l.id}`}
                className={`self-start border-b-[1.5px] text-[15px] ${l.kind === "project" ? "text-slot-6 border-current/45" : "text-accent border-current/40"}`}
              >
                {l.kind === "project" ? `@${l.title}` : l.title}
              </Link>
            ) : (
              <span key={`${l.kind}:${l.title}`} className="text-text-quiet self-start border-b-[1.5px] border-dashed border-current text-[15px]">
                {l.kind === "project" ? `@${l.title}` : l.title}
              </span>
            ),
          )}
        </section>
        <p className="border-rule text-text-quiet rounded-md border-dashed p-2.5 font-mono text-[11px] leading-relaxed break-all">{note.path}</p>
      </aside>
    </div>
  );
}

function TitleInput({ detail, readOnly, onRename }: { detail: NoteDetail; readOnly: boolean; onRename: (title: string) => void }) {
  const [title, setTitle] = useState(detail.note.title);
  useEffect(() => setTitle(detail.note.title), [detail.note.title]);
  const commit = () => {
    const t = title.trim();
    if (!t) setTitle(detail.note.title);
    else if (t !== detail.note.title) onRename(t);
  };
  return (
    <input
      aria-label="Title"
      value={title}
      readOnly={readOnly}
      onChange={(e) => setTitle(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
      className="font-display w-full min-w-0 bg-transparent text-[clamp(32px,5vw,56px)] leading-[0.98] font-extrabold tracking-[-0.04em] outline-none"
    />
  );
}

function MetaRow({
  detail,
  readOnly,
  onChange,
}: {
  detail: NoteDetail;
  readOnly: boolean;
  onChange: (change: { project_ids?: string[]; tags?: string[] }) => void;
}) {
  const { note } = detail;
  const projects = useCanvas(false).data?.projects ?? [];
  const [tagDraft, setTagDraft] = useState("");
  const ids = note.projects.map((p) => p.id);
  // Filing a note under a project shares it with that project's people, so only projects you can edit.
  const others = readOnly ? [] : projects.filter((p) => !ids.includes(p.id) && p.role !== "viewer");

  return (
    <div className="flex flex-wrap items-center gap-2">
      {note.projects.map((p) => (
        <span key={p.id} className="label-mono border-rule inline-flex items-center rounded-sm border-current text-[10.5px]" style={{ color: projectColor(p.color) }}>
          <Link to={`/projects/${p.id}`} className={`py-1 pl-1.5 ${readOnly ? "pr-1.5" : ""}`}>{p.title}</Link>
          {!readOnly && <button type="button" aria-label={`Remove ${p.title}`} onClick={() => onChange({ project_ids: ids.filter((i) => i !== p.id) })} className="cursor-pointer px-1.5 py-1">×</button>}
        </span>
      ))}
      {others.length > 0 && (
        <select
          aria-label="Add a project"
          value=""
          onChange={(e) => e.target.value && onChange({ project_ids: [...ids, e.target.value] })}
          className="label-mono border-rule text-text-quiet min-h-8 rounded-sm border-dashed bg-transparent px-1.5 text-[10.5px] outline-none"
        >
          <option value="">+ Project</option>
          {others.map((p) => (
            <option key={p.id} value={p.id}>{p.title}</option>
          ))}
        </select>
      )}
      {note.tags.map((t) => (
        <span key={t} className="label-mono text-text-quiet inline-flex items-center text-[11px]">
          <Link to={`/notes?tag=${encodeURIComponent(t)}`} className="hover:text-text py-1">#{t}</Link>
          {!readOnly && <button type="button" aria-label={`Remove tag ${t}`} onClick={() => onChange({ tags: note.tags.filter((x) => x !== t) })} className="hover:text-text cursor-pointer px-1 py-1">×</button>}
        </span>
      ))}
      {!readOnly && <form
        onSubmit={(e) => {
          e.preventDefault();
          const added = parseTags(tagDraft);
          if (added.length) onChange({ tags: [...note.tags, ...added] });
          setTagDraft("");
        }}
      >
        <input
          aria-label="Add a tag"
          value={tagDraft}
          onChange={(e) => setTagDraft(e.target.value)}
          placeholder="#tag"
          className="label-mono text-text min-h-8 w-20 bg-transparent px-1 text-[11px] outline-none placeholder:text-text-quiet focus:w-32"
        />
      </form>}
    </div>
  );
}
