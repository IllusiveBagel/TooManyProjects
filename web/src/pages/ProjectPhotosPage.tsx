import { useState } from "react";
import { useProjectTasks } from "../tasks/queries";
import { ErrorNote } from "../components/ui";
import { CAMERA_ICON, datedLabel, DropArea, FileRow, Icon, Lightbox, PickButton, Thumb } from "../files/Files";
import { byWeek, type Attachment } from "../files/format";
import { useAttachments, useUpload } from "../files/queries";
import { useProject } from "./ProjectPage";

const chip = (on: boolean) =>
  `label-mono border-rule min-h-10 cursor-pointer rounded-md px-3 text-[11px] ${on ? "border-text text-text" : "text-text-quiet hover:text-text"}`;

/**
 * A project's photos as a build log: newest first, grouped by week, each
 * tile saying which task it came from. Phones get a camera button.
 */
export function ProjectPhotosPage() {
  const { detail, canEdit } = useProject();
  const { project, children } = detail;
  const [taskId, setTaskId] = useState("");
  const [withChildren, setWithChildren] = useState(false);
  const [photosOnly, setPhotosOnly] = useState(false);
  const [open, setOpen] = useState<string | null>(null);
  const tasks = useProjectTasks(project.id).data ?? [];
  const list = useAttachments(
    taskId ? { task_id: taskId } : { project_id: project.id, include_children: withChildren },
  );
  const upload = useUpload({ project_id: project.id });

  const all = list.data ?? [];
  const images = all.filter((a) => a.is_image);
  const files = all.filter((a) => !a.is_image);
  const shown = photosOnly ? images : all;
  const weeks = byWeek(shown);
  // The lightbox walks through photos in the order they're shown.
  const ordered = weeks.flatMap((w) => w.items.filter((a) => a.is_image));
  const openIndex = open ? ordered.findIndex((a) => a.id === open) : -1;
  const childLabel =
    children.length === 1 ? `Include ${children[0]!.title}` : `Include ${children.length} spawned projects`;
  const add = (f: File[]) => upload.mutate(f);

  return (
    <DropArea onFiles={add} disabled={!canEdit} hint={`Drop photos to add them to ${project.title}`} className="flex-1">
      <div className="flex flex-col gap-5 px-4 py-4 pb-28 sm:px-5 sm:pb-6">
        <div className="flex flex-wrap items-center gap-2">
          <label className="relative">
            <span className="sr-only">Task</span>
            <select
              className={`${chip(!!taskId)} border-text text-text appearance-none pr-8`}
              value={taskId}
              onChange={(e) => setTaskId(e.target.value)}
            >
              <option value="">All tasks</option>
              {tasks
                .filter((t) => t.attachment_count > 0 || t.id === taskId)
                .map((t) => (
                  <option key={t.id} value={t.id}>
                    T-{t.number} {t.title} ({t.attachment_count})
                  </option>
                ))}
            </select>
            <span aria-hidden="true" className="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-[10px]">▾</span>
          </label>
          {children.length > 0 && !taskId && (
            <button type="button" aria-pressed={withChildren} className={`${chip(withChildren)} max-sm:hidden`} onClick={() => setWithChildren(!withChildren)}>
              {withChildren ? "✓ " : ""}
              {childLabel}
            </button>
          )}
          <button type="button" aria-pressed={photosOnly} className={`${chip(photosOnly)} max-sm:hidden`} onClick={() => setPhotosOnly(!photosOnly)}>
            {photosOnly ? "✓ " : ""}Photos only
          </button>
          <span className="label-mono text-text-quiet ml-auto text-[11px]">
            {images.length} photo{images.length === 1 ? "" : "s"}
            {files.length > 0 && ` · ${files.length} file${files.length === 1 ? "" : "s"}`}
          </span>
          {canEdit && (
            <PickButton camera onFiles={add} className="min-h-10 px-3 text-sm max-sm:hidden" disabled={upload.isPending}>
              <Icon d={CAMERA_ICON} /> Add photos
            </PickButton>
          )}
        </div>
        <ErrorNote error={list.error ?? upload.error} />
        {upload.isPending && (
          <p className="label-mono text-text-quiet text-[11px]" role="status">
            Uploading {upload.variables?.length}…
          </p>
        )}

        {list.isPending ? (
          <p className="label-mono text-text-quiet">Loading…</p>
        ) : shown.length === 0 ? (
          <Empty canEdit={canEdit} />
        ) : (
          weeks.map((w) => (
            <section key={w.title} className="flex flex-col gap-3" aria-label={w.title}>
              <h2 className="flex items-baseline gap-3">
                <span className="font-display text-[22px] font-extrabold tracking-[-0.02em]">{w.title}</span>
                <span className="label-mono text-text-quiet text-[11px]">{w.range}</span>
              </h2>
              <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-3 lg:grid-cols-5 xl:grid-cols-6">
                {w.items
                  .filter((a) => a.is_image)
                  .map((a) => (
                    <Thumb key={a.id} a={a} className="aspect-[3/2]" label={datedLabel(a)} onOpen={() => setOpen(a.id)} />
                  ))}
              </div>
              <FilesOf items={w.items.filter((a) => !a.is_image)} />
            </section>
          ))
        )}
      </div>
      {canEdit && (
        <PickButton
          camera
          variant="primary"
          label="Take a photo"
          onFiles={add}
          disabled={upload.isPending}
          className="fixed right-4 bottom-[calc(5.5rem+env(safe-area-inset-bottom))] z-20 size-16 rounded-lg p-0 sm:hidden"
        >
          <Icon d={CAMERA_ICON} className="size-6" />
        </PickButton>
      )}
      {openIndex >= 0 && <Lightbox items={ordered} start={openIndex} onClose={() => setOpen(null)} canCover={canEdit} />}
    </DropArea>
  );
}

function FilesOf({ items }: { items: Attachment[] }) {
  if (!items.length) return null;
  return (
    <div className="grid gap-2 lg:grid-cols-2">
      {items.map((a) => (
        <div key={a.id} className="flex flex-col gap-0.5">
          <FileRow a={a} />
          {(a.task_number || a.note_title) && (
            <span className="label-mono text-text-quiet pl-1 text-[10px]">
              {a.task_number ? `T-${a.task_number} ${a.task_title}` : `Note: ${a.note_title}`}
            </span>
          )}
        </div>
      ))}
    </div>
  );
}

function Empty({ canEdit }: { canEdit: boolean }) {
  return (
    <div className="border-rule text-text-quiet flex flex-col items-center gap-2 rounded-lg border-dashed px-6 py-12 text-center">
      <Icon d={CAMERA_ICON} className="size-8" />
      <p className="font-display text-text text-lg font-extrabold">No photos yet</p>
      <p className="max-w-sm text-sm">
        {canEdit
          ? "Snap progress from a task or its log, or drop photos here. They show up as a build log, week by week."
          : "Photos added to this project and its tasks will show here."}
      </p>
    </div>
  );
}
