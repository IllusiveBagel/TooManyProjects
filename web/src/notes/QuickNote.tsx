import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { create } from "zustand";
import { useCanvas } from "../canvas/queries";
import { Modal } from "../components/Modal";
import { Button, ErrorNote } from "../components/ui";
import { useCreateNote } from "./queries";

interface QuickNoteState {
  open: boolean;
  /** Project to file the note under, when opened from a project. */
  projectId?: string;
  start: (projectId?: string) => void;
  close: () => void;
}

export const useQuickNote = create<QuickNoteState>((set) => ({
  open: false,
  start: (projectId) => set({ open: true, projectId }),
  close: () => set({ open: false, projectId: undefined }),
}));

/** Alt+N anywhere opens Quick note. (Chrome keeps Ctrl+Shift+N for itself.) */
export function useQuickNoteShortcut() {
  const start = useQuickNote((s) => s.start);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.altKey && !e.ctrlKey && !e.metaKey && e.code === "KeyN") {
        e.preventDefault();
        start();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [start]);
}

/** Capture a thought into the Inbox without leaving the page. */
export function QuickNoteDialog() {
  const { open, projectId, close } = useQuickNote();
  if (!open) return null;
  return <QuickNoteForm key={projectId ?? "inbox"} projectId={projectId} onClose={close} />;
}

function QuickNoteForm({ projectId, onClose }: { projectId?: string; onClose: () => void }) {
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [project, setProject] = useState(projectId ?? "");
  const projects = useCanvas(false).data?.projects ?? [];
  const create = useCreateNote();
  const navigate = useNavigate();

  const save = (openAfter: boolean) => {
    if (!title.trim() && !body.trim()) return onClose();
    create.mutate(
      { title: title.trim() || undefined, body, folder: project ? undefined : "inbox", project_ids: project ? [project] : [] },
      {
        onSuccess: (n) => {
          onClose();
          if (openAfter) navigate(`/notes/${n.note.id}`);
        },
      },
    );
  };

  return (
    <Modal title="Quick note" onClose={onClose}>
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          save(false);
        }}
      >
        <textarea
          aria-label="Note text"
          value={body}
          onChange={(e) => setBody(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
              e.preventDefault();
              save(false);
            }
          }}
          placeholder="What's on your mind? [[links]] work here too."
          rows={6}
          className="border-rule bg-surface font-reading min-h-36 rounded-md p-3 text-[17px] outline-none focus:border-accent"
        />
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <label className="flex flex-col gap-1.5">
            <span className="label-mono text-text-quiet">Title (optional)</span>
            <input
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="Date and time"
              className="border-rule bg-surface min-h-11 rounded-md px-3 font-mono text-[13px] outline-none focus:border-accent"
            />
          </label>
          <label className="flex flex-col gap-1.5">
            <span className="label-mono text-text-quiet">File under</span>
            <select
              value={project}
              onChange={(e) => setProject(e.target.value)}
              className="border-rule bg-surface min-h-11 rounded-md px-2 font-mono text-[13px] outline-none focus:border-accent"
            >
              <option value="">Inbox</option>
              {projects.map((p) => (
                <option key={p.id} value={p.id}>{p.title}</option>
              ))}
            </select>
          </label>
        </div>
        <ErrorNote error={create.error} />
        <div className="flex flex-wrap items-center justify-end gap-2">
          <span className="label-mono text-text-quiet mr-auto hidden text-[10px] sm:inline">Ctrl Enter saves · Alt N opens this</span>
          <Button type="button" onClick={() => save(true)} disabled={create.isPending}>Save and open</Button>
          <Button type="submit" variant="primary" disabled={create.isPending}>Save</Button>
        </div>
      </form>
    </Modal>
  );
}
