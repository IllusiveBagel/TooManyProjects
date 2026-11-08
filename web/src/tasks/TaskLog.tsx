import { useState, type ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import { Link, useNavigate } from "react-router";
import remarkGfm from "remark-gfm";
import type { Comment, Task, TaskDetail } from "../api/client";
import { lastActive } from "../canvas/format";
import { Button, ErrorNote, ViaBadge } from "../components/ui";
import { Popover } from "../data/Cell";
import { ago } from "../notes/format";
import { useCreateNote, useNotes } from "../notes/queries";
import { projectColor } from "../theme/resolve";
import { taskRef } from "./format";
import { useComments, useTaskNoteLinks } from "./queries";

const boxClass = "border-rule bg-surface text-text w-full rounded-md px-3 py-2.5 outline-none focus:border-accent";

/** Notes linked to a task: what was learnt, kept in the vault. */
export function TaskNotes({ detail, canEdit }: { detail: TaskDetail; canEdit: boolean }) {
  const { task, notes, hidden_notes } = detail;
  const [picking, setPicking] = useState(false);
  const links = useTaskNoteLinks();
  const create = useCreateNote();
  const navigate = useNavigate();
  const newNote = (title: string) =>
    create.mutate({ title, task_id: task.id }, { onSuccess: (n) => navigate(`/notes/${n.note.id}?edit=1`) });
  if (!canEdit && notes.length === 0 && hidden_notes === 0) return null;

  return (
    <section id="task-notes" className="flex scroll-mt-4 flex-col gap-1.5" aria-label="Linked notes">
      <h3 className="label-mono text-text-quiet">
        Linked notes {notes.length + hidden_notes > 0 && <span className="text-accent">{notes.length + hidden_notes}</span>}
      </h3>
      <div className="border-rule bg-surface relative rounded-md">
        <ul className="flex flex-col">
          {notes.map((n) => (
            <li key={n.id} className="border-rule group flex items-stretch border-x-0 border-t-0">
              <Link
                to={`/notes/${n.id}`}
                className="hover:bg-surface-raised flex min-h-11 min-w-0 flex-1 flex-col justify-center px-3 py-2"
              >
                <span className="flex items-baseline gap-2">
                  <span className="font-display text-accent min-w-0 flex-1 truncate text-[15px] font-extrabold tracking-[-0.01em]">
                    {n.title}
                  </span>
                  <span className="label-mono text-text-quiet shrink-0 text-[10px]">{ago(n.updated_at)}</span>
                </span>
                {n.excerpt && <span className="text-text-quiet truncate text-[13px]">{n.excerpt}</span>}
              </Link>
              {canEdit && (
                <button
                  type="button"
                  onClick={() => links.unlink.mutate({ taskId: task.id, noteId: n.id })}
                  aria-label={`Unlink ${n.title}`}
                  title="Unlink (keeps the note)"
                  className="text-text-quiet hover:text-late w-10 shrink-0 cursor-pointer sm:opacity-0 sm:group-hover:opacity-100 sm:focus:opacity-100"
                >
                  ×
                </button>
              )}
            </li>
          ))}
        </ul>
        {hidden_notes > 0 && (
          <p className="label-mono text-text-quiet border-rule border-x-0 border-t-0 px-3 py-2 text-[10.5px]">
            + {hidden_notes} note{hidden_notes === 1 ? "" : "s"} not shared with you
          </p>
        )}
        {canEdit && (
          <div className="flex">
            <button
              type="button"
              className="label-mono text-text-quiet hover:text-text min-h-10 flex-1 cursor-pointer px-3 text-left text-[11px]"
              aria-expanded={picking}
              onClick={() => setPicking(true)}
            >
              + Link a note
            </button>
            <button
              type="button"
              className="label-mono text-text-quiet hover:text-text border-rule min-h-10 cursor-pointer border-y-0 border-r-0 px-3 text-[11px]"
              onClick={() => newNote(`${taskRef(task)} ${task.title}`)}
              disabled={create.isPending}
            >
              + New note
            </button>
          </div>
        )}
        {picking && (
          <NotePicker
            task={task}
            linked={notes.map((n) => n.id)}
            onPick={(noteId) => links.link.mutate({ taskId: task.id, noteId }, { onSuccess: () => setPicking(false) })}
            onCreate={(title) => newNote(title)}
            onClose={() => setPicking(false)}
          />
        )}
      </div>
      <ErrorNote error={links.link.error ?? links.unlink.error ?? create.error} />
    </section>
  );
}

function NotePicker({
  task,
  linked,
  onPick,
  onCreate,
  onClose,
}: {
  task: Task;
  linked: string[];
  onPick: (noteId: string) => void;
  onCreate: (title: string) => void;
  onClose: () => void;
}) {
  const [q, setQ] = useState("");
  // With no words typed, the project's own notes come first.
  const found = useNotes(q.trim() ? { q: q.trim(), limit: 30 } : { project_id: task.project_id, limit: 30 });
  const recent = useNotes({ limit: 30 });
  const seen = new Set(linked);
  const list = [...(found.data ?? []), ...(q.trim() ? [] : (recent.data ?? []))].filter((n) => {
    if (seen.has(n.id)) return false;
    seen.add(n.id);
    return true;
  });
  return (
    <Popover label="Link a note" onClose={onClose} wide>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (list[0]) onPick(list[0].id);
          else if (q.trim()) onCreate(q.trim());
        }}
      >
        <input
          autoFocus
          className="border-rule bg-surface mb-1 min-h-10 w-full rounded-sm px-2 font-mono text-[13px] outline-none focus:border-accent"
          placeholder="find a note, or name a new one"
          aria-label="Find a note"
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
      </form>
      {q.trim() && (
        <button
          type="button"
          className="hover:bg-surface-raised min-h-10 cursor-pointer rounded-sm px-2 text-left text-sm"
          onClick={() => onCreate(q.trim())}
        >
          + New note “{q.trim()}”
        </button>
      )}
      {list.slice(0, 40).map((n) => (
        <button
          key={n.id}
          type="button"
          className="hover:bg-surface-raised flex min-h-10 cursor-pointer items-baseline gap-2 rounded-sm px-2 text-left text-sm"
          onClick={() => onPick(n.id)}
        >
          <span className="min-w-0 flex-1 truncate">{n.title}</span>
          {n.projects[0] && (
            <span className="label-mono shrink-0 text-[10px]" style={{ color: projectColor(n.projects[0].color) }}>
              {n.projects[0].title}
            </span>
          )}
        </button>
      ))}
      {list.length === 0 && !q.trim() && <p className="text-text-quiet px-2 py-2 text-sm">No notes yet.</p>}
    </Popover>
  );
}

/** The task's log: comments, oldest first, with a box to add one. */
export function TaskLog({ detail, canEdit }: { detail: TaskDetail; canEdit: boolean }) {
  const { task, comments } = detail;
  const log = useComments(task.id);
  const [draft, setDraft] = useState("");
  const submit = () => {
    if (draft.trim()) log.add.mutate(draft.trim(), { onSuccess: () => setDraft("") });
  };
  if (!canEdit && comments.length === 0) return null;

  return (
    <section id="task-log" className="flex scroll-mt-4 flex-col gap-2" aria-label="Log">
      <h3 className="label-mono text-text-quiet">
        Log {comments.length > 0 && <span className="text-accent">{comments.length}</span>}
      </h3>
      {comments.length > 0 && (
        <ol className="border-rule flex flex-col gap-4 border-y-0 border-r-0 pl-3.5">
          {comments.map((c) => (
            <CommentItem key={c.id} comment={c} onSave={(body) => log.update.mutateAsync({ id: c.id, body })} onDelete={() => log.remove.mutate(c.id)} />
          ))}
        </ol>
      )}
      {canEdit && (
        <form
          className="flex flex-col gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            submit();
          }}
        >
          <textarea
            className={`${boxClass} font-reading min-h-20 text-[15px] leading-relaxed`}
            value={draft}
            maxLength={20000}
            placeholder={comments.length ? "What did you do or find out?" : "Log what you did, tried or learnt…"}
            aria-label="New comment"
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                e.preventDefault();
                submit();
              }
            }}
          />
          <div className="flex items-center gap-3">
            <span className="label-mono text-text-quiet hidden text-[10px] sm:inline">Markdown · Ctrl Enter</span>
            <Button type="submit" variant="primary" className="ml-auto min-h-10 px-3 text-sm" disabled={!draft.trim() || log.add.isPending}>
              Add to log
            </Button>
          </div>
        </form>
      )}
      <ErrorNote error={log.add.error ?? log.update.error ?? log.remove.error} />
    </section>
  );
}

function ExternalLink({ href, children }: { href?: string; children?: ReactNode }) {
  return (
    <a href={href} target="_blank" rel="noreferrer noopener" className="text-accent underline decoration-1 underline-offset-4">
      {children}
    </a>
  );
}

function CommentItem({
  comment: c,
  onSave,
  onDelete,
}: {
  comment: Comment;
  onSave: (body: string) => Promise<unknown>;
  onDelete: () => void;
}) {
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(c.body);
  const [confirming, setConfirming] = useState(false);
  return (
    <li className="relative flex flex-col gap-1">
      <span aria-hidden="true" className="bg-accent absolute top-1.5 -left-[19px] size-2 rounded-[2px]" />
      <span className="label-mono text-text-quiet flex flex-wrap items-center gap-x-2 text-[10.5px]">
        <b className="text-text font-semibold">{c.user_name ?? "Someone"}</b>
        <time dateTime={c.created_at} title={new Date(c.created_at).toLocaleString()}>{lastActive(c.created_at)}</time>
        {c.edited_at && <span title={`Edited ${new Date(c.edited_at).toLocaleString()}`}>· edited</span>}
        <ViaBadge via={c.via} />
        {!editing && (c.can_edit || c.can_delete) && (
          <span className="ml-auto flex gap-1">
            {c.can_edit && (
              <button type="button" className="hover:text-text min-h-8 cursor-pointer px-1" onClick={() => (setText(c.body), setEditing(true))}>
                Edit
              </button>
            )}
            {c.can_delete &&
              (confirming ? (
                <>
                  <button type="button" className="text-late min-h-8 cursor-pointer px-1" onClick={onDelete}>Delete it</button>
                  <button type="button" className="hover:text-text min-h-8 cursor-pointer px-1" onClick={() => setConfirming(false)}>Keep</button>
                </>
              ) : (
                <button type="button" className="hover:text-late min-h-8 cursor-pointer px-1" onClick={() => setConfirming(true)}>
                  Delete
                </button>
              ))}
          </span>
        )}
      </span>
      {editing ? (
        <form
          className="flex flex-col gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (text.trim()) onSave(text.trim()).then(() => setEditing(false));
          }}
        >
          <textarea
            autoFocus
            className={`${boxClass} font-reading min-h-20 text-[15px] leading-relaxed`}
            value={text}
            maxLength={20000}
            aria-label="Edit comment"
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) e.currentTarget.form?.requestSubmit();
              if (e.key === "Escape") (e.stopPropagation(), setEditing(false));
            }}
          />
          <div className="flex justify-end gap-2">
            <Button type="button" className="min-h-9 px-3 text-sm" onClick={() => setEditing(false)}>Cancel</Button>
            <Button type="submit" variant="primary" className="min-h-9 px-3 text-sm" disabled={!text.trim()}>Save</Button>
          </div>
        </form>
      ) : (
        <div className="note-prose comment-prose">
          <ReactMarkdown remarkPlugins={[remarkGfm]} components={{ a: ExternalLink }}>
            {c.body}
          </ReactMarkdown>
        </div>
      )}
    </li>
  );
}
