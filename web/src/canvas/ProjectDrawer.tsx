import { useEffect, useState } from "react";
import { Link } from "react-router";
import type { ProjectDetail } from "../api/client";
import { ColorPicker, StatusPicker } from "../components/Pickers";
import { Button, ErrorNote } from "../components/ui";
import { SharedNote } from "../sharing/SharedNote";
import { ApplyTemplateDialog } from "../templates/ApplyTemplateDialog";
import { SaveTemplateDialog } from "../templates/SaveTemplateDialog";
import { projectColor } from "../theme/resolve";
import { lastActive, progress } from "./format";
import { useDeleteProject, useProjectDetail, useUpdateProject } from "./queries";
import { useCanvasUi } from "./store";

const inputClass =
  "border-rule bg-surface text-text w-full rounded-md px-3 py-2.5 outline-none focus:border-accent";

/** Side panel to edit the open project. Each field saves when it loses focus. */
export function ProjectDrawer({ onJump }: { onJump: (projectId: string) => void }) {
  const id = useCanvasUi((s) => s.openProjectId);
  const openProject = useCanvasUi((s) => s.openProject);
  const detail = useProjectDetail(id);

  useEffect(() => {
    if (!id) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement))
        openProject(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [id, openProject]);

  if (!id) return null;
  return (
    <aside
      aria-label="Project"
      className="border-rule bg-surface-card fixed inset-x-0 bottom-0 z-40 flex max-h-[85vh] flex-col overflow-hidden rounded-t-lg border-b-0 sm:absolute sm:inset-y-0 sm:right-0 sm:left-auto sm:max-h-none sm:w-[400px] sm:rounded-none sm:border-y-0 sm:border-r-0"
    >
      {detail.data ? (
        // Re-mount per project so the form starts from that project's values.
        <DrawerBody key={detail.data.project.id} detail={detail.data} onJump={onJump} />
      ) : detail.isError ? (
        <div className="p-5"><ErrorNote error={detail.error} /></div>
      ) : (
        <p className="label-mono text-text-quiet p-5">Loading…</p>
      )}
    </aside>
  );
}

function DrawerBody({ detail, onJump }: { detail: ProjectDetail; onJump: (id: string) => void }) {
  const { project, parents, children } = detail;
  const openProject = useCanvasUi((s) => s.openProject);
  const update = useUpdateProject();
  const remove = useDeleteProject();
  const [title, setTitle] = useState(project.title);
  const [summary, setSummary] = useState(project.summary ?? "");
  const [description, setDescription] = useState(project.description ?? "");
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [templateDialog, setTemplateDialog] = useState<"save" | "apply" | null>(null);
  const save = (fields: Parameters<typeof update.mutate>[0]) => update.mutate(fields);
  const archived = project.status === "archived";
  const color = projectColor(project.color);
  const canEdit = project.role !== "viewer";

  return (
    <>
      <div className="flex items-start gap-3 border-t-[6px] px-5 pt-4" style={{ borderTopColor: color }}>
        <label className="flex-1">
          <span className="sr-only">Title</span>
          <input
            className="font-display text-text w-full bg-transparent text-[26px] leading-tight font-extrabold tracking-[-0.02em] outline-none"
            value={title}
            maxLength={200}
            readOnly={!canEdit}
            onChange={(e) => setTitle(e.target.value)}
            onBlur={() => title.trim() && title !== project.title && save({ id: project.id, title })}
          />
        </label>
        <button
          type="button"
          onClick={() => openProject(null)}
          aria-label="Close"
          className="text-text-quiet hover:text-text grid size-11 shrink-0 cursor-pointer place-items-center text-2xl"
        >
          ×
        </button>
      </div>

      <div className="flex flex-1 flex-col gap-5 overflow-y-auto px-5 pt-2 pb-6">
        <SharedNote project={project} />
        <div className="label-mono text-text-quiet grid grid-cols-3 gap-2">
          <Stat label="Done" value={`${project.stats.done_tasks}/${project.stats.total_tasks}`} />
          <Stat label="Progress" value={`${progress(project)}%`} />
          <Stat label="Active" value={lastActive(project.stats.last_active_at)} />
        </div>

        <div className="flex gap-2">
          <Link
            to={`/projects/${project.id}`}
            className="border-rule bg-accent text-on-accent shadow-button inline-flex min-h-11 flex-1 items-center justify-center rounded-md px-3 font-bold"
          >
            Open dashboard
          </Link>
          <Link
            to={`/projects/${project.id}/board`}
            className="border-rule hover:bg-surface-raised inline-flex min-h-11 flex-1 items-center justify-center rounded-md px-3 font-bold"
          >
            Board
          </Link>
        </div>

        {(parents.length > 0 || children.length > 0) && (
          <div className="flex flex-col gap-2">
            <Family label="Spawned from" items={parents} onJump={onJump} />
            <Family label="Spawned" items={children} onJump={onJump} />
          </div>
        )}

        {/* A viewer sees the same fields, read-only. */}
        <fieldset disabled={!canEdit} className="flex min-w-0 flex-col gap-5">
          <label className="flex flex-col gap-1.5">
            <span className="label-mono text-text-quiet">Summary</span>
            <input
              className={inputClass}
              value={summary}
              maxLength={500}
              placeholder="One line on what this is"
              onChange={(e) => setSummary(e.target.value)}
              onBlur={() => summary !== (project.summary ?? "") && save({ id: project.id, summary: summary || null })}
            />
          </label>

          <StatusPicker value={project.status} includeArchived onChange={(status) => save({ id: project.id, status })} />
          <ColorPicker value={project.color} onChange={(c) => save({ id: project.id, color: c })} />

          <label className="flex flex-col gap-1.5">
            <span className="label-mono text-text-quiet">Target date</span>
            <input
              type="date"
              className={`${inputClass} font-mono text-[13px]`}
              value={project.target_date ?? ""}
              onChange={(e) => save({ id: project.id, target_date: e.target.value || null })}
            />
          </label>

          <label className="flex flex-col gap-1.5">
            <span className="label-mono text-text-quiet">Description · markdown</span>
            <textarea
              className={`${inputClass} font-reading min-h-40 text-[15px] leading-relaxed`}
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              onBlur={() =>
                description !== (project.description ?? "") && save({ id: project.id, description: description || null })
              }
            />
          </label>
        </fieldset>

        <ErrorNote error={update.error ?? remove.error} />

        <div className="border-rule flex flex-wrap gap-2 border-x-0 border-b-0 pt-4">
          <Button type="button" onClick={() => setTemplateDialog("save")}>
            Save as template…
          </Button>
          {canEdit && project.stats.total_tasks === 0 && (
            <Button type="button" onClick={() => setTemplateDialog("apply")}>
              Use a template…
            </Button>
          )}
        </div>
        {templateDialog === "save" && <SaveTemplateDialog project={project} onClose={() => setTemplateDialog(null)} />}
        {templateDialog === "apply" && <ApplyTemplateDialog project={project} onClose={() => setTemplateDialog(null)} />}

        {canEdit && (
          <div className="flex flex-wrap gap-2">
            <Button type="button" onClick={() => save({ id: project.id, status: archived ? "active" : "archived" })}>
              {archived ? "Restore" : "Archive"}
            </Button>
            {project.role !== "owner" ? null : confirmDelete ? (
              <>
                <Button
                  type="button"
                  className="text-late border-current"
                  onClick={() => remove.mutate(project.id, { onSuccess: () => openProject(null) })}
                >
                  Delete for good
                </Button>
                <Button type="button" onClick={() => setConfirmDelete(false)}>Keep it</Button>
              </>
            ) : (
              <Button type="button" className="text-late" onClick={() => setConfirmDelete(true)}>
                Delete…
              </Button>
            )}
          </div>
        )}
        {confirmDelete && (
          <p className="text-text-quiet text-sm">
            Deleting removes the project, its tasks and its links. Archiving hides it and keeps everything.
          </p>
        )}
      </div>
    </>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="border-rule bg-surface rounded-md px-2.5 py-2">
      <span className="block text-[10px]">{label}</span>
      <span className="text-text font-mono text-sm normal-case">{value}</span>
    </div>
  );
}

function Family({ label, items, onJump }: { label: string; items: ProjectDetail["parents"]; onJump: (id: string) => void }) {
  if (items.length === 0) return null;
  return (
    <div className="flex flex-wrap items-center gap-1.5">
      <span className="label-mono text-text-quiet mr-1">{label}</span>
      {items.map((p) => (
        <button
          key={p.id}
          type="button"
          onClick={() => onJump(p.id)}
          className="border-rule hover:border-accent min-h-9 cursor-pointer rounded-sm px-2 text-sm font-bold"
          style={{ borderLeftColor: projectColor(p.color), borderLeftWidth: 4 }}
        >
          {p.title}
        </button>
      ))}
    </div>
  );
}
