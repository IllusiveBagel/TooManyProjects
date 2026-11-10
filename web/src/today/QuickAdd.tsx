import { useEffect, useMemo, useRef, useState } from "react";
import { Link, useLocation } from "react-router";
import { create } from "zustand";
import { api, unwrap } from "../api/client";
import { useCanvas } from "../canvas/queries";
import { Modal } from "../components/Modal";
import { Button, ErrorNote } from "../components/ui";
import { CAMERA_ICON, Icon, PickButton } from "../files/Files";
import { uploadFiles } from "../files/queries";
import type { components } from "../api/schema";
import { projectColor } from "../theme/resolve";
import { useStatuses } from "../tasks/queries";
import { findUrl, lastProject, linkTitle, rememberProject, shortUrl, type QuickKind } from "./format";
import { refreshCaptures, useFileInbox, useLinkPreview, useQuickAdd } from "./queries";
import { useQueryClient } from "@tanstack/react-query";

type NoteSummary = components["schemas"]["NoteSummary"];

export interface QuickAddStart {
  text?: string;
  url?: string | null;
  body?: string;
  files?: File[];
  kind?: QuickKind;
  projectId?: string;
  /** Where it came from, shown in the corner: "Shared from your phone". */
  source?: string;
  /** Filing this Inbox capture rather than adding something new. */
  inbox?: NoteSummary;
}

interface Added {
  text: string;
  path: string;
}

interface QuickAddState {
  open: QuickAddStart | null;
  /** Bumps on every open, so the form starts fresh. */
  serial: number;
  added: Added | null;
  start: (s?: QuickAddStart) => void;
  close: () => void;
  done: (a: Added) => void;
  clearAdded: () => void;
}

export const useQuickAddUi = create<QuickAddState>((set) => ({
  open: null,
  serial: 0,
  added: null,
  start: (s = {}) => set((st) => ({ open: s, serial: st.serial + 1 })),
  close: () => set({ open: null }),
  done: (added) => set({ added, open: null }),
  clearAdded: () => set({ added: null }),
}));

/** Alt+A anywhere opens Quick add. Uses the key's position, so a Mac's Option+A works too. */
export function useQuickAddShortcut() {
  const start = useQuickAddUi((s) => s.start);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.altKey && !e.ctrlKey && !e.metaKey && e.code === "KeyA") {
        e.preventDefault();
        start();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [start]);
}

/** The project a page is about, from its address. */
function routeProject(pathname: string): string | undefined {
  return /^\/projects\/([^/]+)/.exec(pathname)?.[1];
}

export function QuickAddDialog() {
  const { open, serial } = useQuickAddUi();
  if (!open) return null;
  return <QuickAddForm key={serial} start={open} />;
}

/** The note after its quick-add body, with photo embeds added. */
async function embedPhotos(noteId: string, names: string[]) {
  if (names.length === 0) return;
  const note = await unwrap(api.GET("/notes/{id}", { params: { path: { id: noteId } } }));
  const embeds = names.map((n) => `![[${n}]]`).join("\n");
  const body = note.body.trim() ? `${note.body.trimEnd()}\n\n${embeds}\n` : `${embeds}\n`;
  await unwrap(api.PATCH("/notes/{id}", { params: { path: { id: noteId } }, body: { body } }));
}

const kindLabel: Record<QuickKind, string> = { task: "Task", note: "Note", inbox: "Inbox" };

function QuickAddForm({ start }: { start: QuickAddStart }) {
  const { close, done } = useQuickAddUi();
  const { pathname } = useLocation();
  const filing = start.inbox;
  const projects = (useCanvas(false).data?.projects ?? []).filter((p) => p.role !== "viewer");
  const [text, setText] = useState(start.text ?? filing?.title ?? "");
  const [url, setUrl] = useState<string | null>(start.url ?? null);
  const [files, setFiles] = useState<File[]>(start.files ?? []);
  const fromRoute = routeProject(pathname);
  const [kind, setKind] = useState<QuickKind>(start.kind ?? (filing ? "task" : "task"));
  const [projectId, setProjectId] = useState(start.projectId ?? fromRoute ?? lastProject());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const touched = useRef(!!start.text);
  const add = useQuickAdd();
  const file = useFileInbox();
  const qc = useQueryClient();

  // Pick a project that exists and can take it.
  const project = projects.find((p) => p.id === projectId) ?? null;
  useEffect(() => {
    if (!project && projects.length > 0 && kind !== "inbox" && !projectId) setProjectId(projects[0]!.id);
  }, [projects.length]);
  const statuses = useStatuses(kind === "task" && project ? project.id : undefined).data ?? [];
  const column = statuses.find((s) => s.category === "todo") ?? statuses.find((s) => s.category !== "done") ?? statuses[0];

  const preview = useLinkPreview(url);
  useEffect(() => {
    if (preview.data && !touched.current && !text.trim()) {
      const t = linkTitle(preview.data);
      if (t) setText(t);
    }
  }, [preview.data]);

  const thumbs = useMemo(() => files.map((f) => (f.type.startsWith("image/") ? URL.createObjectURL(f) : null)), [files]);
  useEffect(() => () => thumbs.forEach((t) => t && URL.revokeObjectURL(t)), [thumbs]);

  const onText = (v: string) => {
    touched.current = true;
    const found = !url ? findUrl(v) : null;
    if (found) {
      setUrl(found);
      // A pasted bare link waits for the page's title instead.
      const rest = v.replace(found, "").trim();
      touched.current = rest.length > 0;
      setText(rest);
      return;
    }
    setText(v);
  };

  const needsProject = kind !== "inbox";
  const canSave = (text.trim() || url) && (!needsProject || project) && !busy;

  const submit = async () => {
    if (!canSave) return;
    setBusy(true);
    setError(null);
    try {
      if (filing) {
        const res = await file.mutateAsync({ id: filing.id, kind: kind === "note" ? "note" : "task", project_id: project!.id, title: text.trim() || undefined });
        if (files.length && res.task) await uploadFiles({ task_id: res.task.id }, files);
        rememberProject(project!.id);
        done({
          text: res.kind === "task" ? `Filed in ${project!.title}` : `Filed under ${project!.title}`,
          path: res.task ? `/projects/${project!.id}/board?task=${res.task.id}` : `/notes/${filing.id}`,
        });
        return;
      }
      const res = await add.mutateAsync({
        text: text.trim(),
        kind,
        project_id: needsProject ? project!.id : null,
        url,
        body: start.body || null,
      });
      if (files.length) {
        if (res.task) await uploadFiles({ task_id: res.task.id }, files);
        else if (res.note_id) {
          const added = await uploadFiles({ note_id: res.note_id }, files);
          await embedPhotos(res.note_id, added.map((a) => a.name));
        }
      }
      if (needsProject) rememberProject(project!.id);
      refreshCaptures(qc);
      done({
        text: res.task ? `Added to ${project!.title} · ${res.column}` : res.kind === "note" ? `Note added to ${project!.title}` : "Added to the Inbox",
        path: res.task ? `/projects/${project!.id}/board?task=${res.task.id}` : `/notes/${res.note_id}`,
      });
    } catch (e) {
      setError(e);
      refreshCaptures(qc);
    } finally {
      setBusy(false);
    }
  };

  const action = filing
    ? kind === "task"
      ? `File in ${column?.name ?? "the board"}`
      : "File under project"
    : kind === "task"
      ? `Add to ${column?.name ?? "the board"}`
      : kind === "note"
        ? "Add note"
        : "Add to Inbox";

  return (
    <Modal title={filing ? "File it" : "Quick add"} onClose={close}>
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        {(start.source || filing) && (
          <p className="label-mono text-text-quiet absolute top-6 right-5 text-[10px]">{start.source ?? "From the Inbox"}</p>
        )}
        <input
          aria-label="What to add"
          value={text}
          onChange={(e) => onText(e.target.value)}
          placeholder={url ? (preview.isFetching ? "Fetching the page's title…" : "Name it") : "An idea, a task, a link…"}
          maxLength={500}
          className="border-rule bg-surface focus:border-accent min-h-13 rounded-md px-3 text-[17px] font-bold outline-none"
        />
        {url && (
          <div className="border-rule flex items-center gap-3 rounded-md p-2.5">
            {preview.data?.image ? (
              <img src={preview.data.image} alt="" referrerPolicy="no-referrer" className="bg-surface-raised size-14 shrink-0 rounded-sm object-cover" />
            ) : (
              <span className="label-mono bg-surface-raised text-text-quiet grid size-14 shrink-0 place-items-center rounded-sm text-[10px]">Link</span>
            )}
            <span className="min-w-0 flex-1">
              <span className="block truncate font-bold">{preview.data?.title ?? (preview.isFetching ? "Looking…" : shortUrl(url, 40))}</span>
              <a href={url} target="_blank" rel="noreferrer noopener" className="label-mono text-text-quiet hover:text-text block truncate text-[10.5px]">
                {shortUrl(url)}
              </a>
            </span>
            <button type="button" onClick={() => setUrl(null)} aria-label="Remove link" className="text-text-quiet hover:text-text grid size-9 cursor-pointer place-items-center">
              ✕
            </button>
          </div>
        )}
        {files.length > 0 && (
          <ul className="flex flex-wrap gap-2" aria-label="Photos to add">
            {files.map((f, i) => (
              <li key={i} className="border-rule relative size-16 overflow-hidden rounded-md">
                {thumbs[i] ? (
                  <img src={thumbs[i]!} alt={f.name} className="size-full object-cover" />
                ) : (
                  <span className="label-mono text-text-quiet grid size-full place-items-center p-1 text-center text-[9px] break-all">{f.name}</span>
                )}
                <button
                  type="button"
                  onClick={() => setFiles(files.filter((_, j) => j !== i))}
                  aria-label={`Remove ${f.name}`}
                  className="absolute top-0.5 right-0.5 grid size-6 cursor-pointer place-items-center rounded-full bg-[rgba(0,0,0,0.7)] text-[11px] text-[#fff]"
                >
                  ✕
                </button>
              </li>
            ))}
          </ul>
        )}

        <div className="flex flex-col gap-1.5">
          <span className="label-mono text-text-quiet">As</span>
          <div role="radiogroup" aria-label="Add as" className={`grid gap-2 ${filing ? "grid-cols-2" : "grid-cols-3"}`}>
            {(filing ? (["task", "note"] as const) : (["task", "note", "inbox"] as const)).map((k) => (
              <button
                key={k}
                type="button"
                role="radio"
                aria-checked={kind === k}
                onClick={() => setKind(k)}
                className={`min-h-12 cursor-pointer rounded-md border font-bold ${kind === k ? "border-accent bg-surface-raised" : "border-line text-text-quiet hover:text-text"}`}
              >
                {kindLabel[k]}
              </button>
            ))}
          </div>
        </div>

        {needsProject && (
          <label className="flex flex-col gap-1.5">
            <span className="label-mono text-text-quiet">In</span>
            <span className="border-rule bg-surface focus-within:border-accent flex min-h-12 items-center gap-2.5 rounded-md px-3">
              <span className="size-3 shrink-0 rounded-[3px]" style={{ background: project ? projectColor(project.color) : "transparent" }} aria-hidden="true" />
              <select
                value={project?.id ?? ""}
                onChange={(e) => setProjectId(e.target.value)}
                className="min-w-0 flex-1 cursor-pointer bg-transparent py-2 font-bold outline-none"
              >
                {!project && <option value="">Pick a project</option>}
                {projects.map((p) => (
                  <option key={p.id} value={p.id}>{p.title}</option>
                ))}
              </select>
            </span>
          </label>
        )}

        <ErrorNote error={error} />
        <div className="flex flex-wrap items-center gap-2">
          <PickButton camera onFiles={(f) => setFiles([...files, ...f])} label="Add a photo" className="min-h-12 px-3">
            <Icon d={CAMERA_ICON} />
            <span>Photo</span>
          </PickButton>
          {filing && (
            <Link to={`/notes/${filing.id}`} onClick={close} className="label-mono text-text-quiet hover:text-text px-2 text-[11px]">
              Open note
            </Link>
          )}
          {!filing && <span className="label-mono text-text-quiet ml-auto hidden text-[10px] lg:inline">Enter adds · Alt A opens</span>}
          <Button type="submit" variant="primary" disabled={!canSave} className="min-h-12 w-full sm:ml-auto sm:w-auto">
            {busy ? "Adding…" : action}
          </Button>
        </div>
      </form>
    </Modal>
  );
}

/** Says where a capture went, with a link to it, for a few seconds. */
export function QuickAddToast() {
  const { added, clearAdded } = useQuickAddUi();
  useEffect(() => {
    if (!added) return;
    const t = window.setTimeout(clearAdded, 5000);
    return () => window.clearTimeout(t);
  }, [added]);
  if (!added) return null;
  return (
    <div
      role="status"
      className="border-rule bg-surface-raised shadow-card fixed bottom-20 left-1/2 z-50 flex max-w-[92vw] -translate-x-1/2 items-center gap-4 rounded-md px-4 py-2.5 sm:bottom-6"
    >
      <span className="truncate font-semibold">{added.text}</span>
      <Link to={added.path} onClick={clearAdded} className="label-mono text-accent min-h-9 shrink-0 content-center px-1 text-[11px]">
        Open
      </Link>
    </div>
  );
}
