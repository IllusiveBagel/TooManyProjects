import { useEffect, useRef, useState, type ReactNode } from "react";
import { Link } from "react-router";
import { Button, ErrorNote, ViaBadge } from "../components/ui";
import { fileKind, fileSize, fileUrl, filesFrom, shortDay, tileLabel, when, type Attachment } from "./format";
import { useDeleteAttachment, useSetCover, useUpdateAttachment, useUpload, type Target } from "./queries";

export const CAMERA_ICON = "M4 8h3l2-3h6l2 3h3v11H4zM12 17a4 4 0 1 0 0-8 4 4 0 0 0 0 8z";
export const CLIP_ICON = "M20 11.5l-8.2 8.2a5 5 0 0 1-7-7l8.5-8.6a3.3 3.3 0 0 1 4.7 4.7l-8.5 8.6a1.7 1.7 0 0 1-2.4-2.4l7.8-7.8";
const FILE_ICON = "M6 3h9l4 4v14H6zM14 3v5h5";

export function Icon({ d, className = "size-4.5" }: { d: string; className?: string }) {
  return (
    <svg viewBox="0 0 24 24" className={className} fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinejoin="round" aria-hidden="true">
      <path d={d} />
    </svg>
  );
}

/**
 * A button that opens the file picker. With `camera`, phones go straight to
 * the camera (desktops just pick images).
 */
export function PickButton({
  onFiles,
  camera,
  children,
  className = "",
  variant,
  disabled,
  label,
}: {
  onFiles: (files: File[]) => void;
  camera?: boolean;
  children: ReactNode;
  className?: string;
  variant?: "primary" | "ghost";
  disabled?: boolean;
  label?: string;
}) {
  const input = useRef<HTMLInputElement>(null);
  return (
    <>
      <Button type="button" variant={variant} className={className} disabled={disabled} aria-label={label} onClick={() => input.current?.click()}>
        {children}
      </Button>
      <input
        ref={input}
        type="file"
        multiple
        hidden
        accept={camera ? "image/*" : undefined}
        capture={camera ? "environment" : undefined}
        onChange={(e) => {
          const files = Array.from(e.target.files ?? []);
          e.target.value = "";
          if (files.length) onFiles(files);
        }}
      />
    </>
  );
}

/** A photo tile: the 400 px preview with a label along the bottom. */
export function Thumb({
  a,
  label,
  onOpen,
  className = "",
  children,
}: {
  a: Attachment;
  label?: string;
  onOpen: () => void;
  className?: string;
  children?: ReactNode;
}) {
  return (
    <div className={`group bg-surface-raised border-rule relative overflow-hidden rounded-md ${className}`}>
      <button type="button" onClick={onOpen} className="block size-full cursor-zoom-in" aria-label={`Open ${a.caption ?? a.name}`}>
        <img src={fileUrl(a, 400)} alt={a.caption ?? ""} loading="lazy" className="size-full object-cover" draggable={false} />
      </button>
      {label && (
        <span className="label-mono pointer-events-none absolute bottom-2 left-2 max-w-[calc(100%-1rem)] truncate rounded-sm bg-[rgba(0,0,0,0.70)] px-1.5 py-1 text-[10px] text-[#fff]">
          {label}
        </span>
      )}
      {children}
    </div>
  );
}

/** A non-image file: icon, name, type and size. Opens (PDF) or downloads. */
export function FileRow({ a, onRemove }: { a: Attachment; onRemove?: () => void }) {
  const inline = a.mime === "application/pdf";
  return (
    <div className="border-rule bg-surface group flex min-h-12 items-center gap-3 rounded-md px-3">
      <Icon d={FILE_ICON} className="text-text-quiet size-5 shrink-0" />
      <a
        href={fileUrl(a, undefined, !inline)}
        target={inline ? "_blank" : undefined}
        rel="noreferrer"
        className="min-w-0 flex-1 truncate py-2 font-semibold hover:underline"
        title={a.caption ?? a.name}
      >
        {a.name}
      </a>
      <span className="label-mono text-text-quiet shrink-0 text-[10.5px]">
        {fileKind(a)} · {fileSize(a.size)}
      </span>
      {onRemove && a.can_edit && (
        <button
          type="button"
          onClick={onRemove}
          aria-label={`Remove ${a.name}`}
          className="text-text-quiet hover:text-late -mr-1 size-9 shrink-0 cursor-pointer sm:opacity-0 sm:group-hover:opacity-100 sm:focus:opacity-100"
        >
          ×
        </button>
      )}
    </div>
  );
}

/**
 * Photos as a grid of tiles, then other files as rows. With `max`, the grid
 * stops at that many and a "+N more" tile opens the rest.
 */
export function FileGrid({
  items,
  max,
  columns = 3,
  labels = "caption",
  projectCanCover,
  onRemove,
}: {
  items: Attachment[];
  max?: number;
  columns?: 2 | 3 | 4;
  /** What tiles say: their caption (or name), or nothing. */
  labels?: "caption" | "none";
  /** Offer "Use as cover" in the lightbox for this project. */
  projectCanCover?: boolean;
  onRemove?: (a: Attachment) => void;
}) {
  const [open, setOpen] = useState<number | null>(null);
  const [all, setAll] = useState(false);
  const images = items.filter((a) => a.is_image);
  const files = items.filter((a) => !a.is_image);
  const cut = max && !all && images.length > max ? max - 1 : images.length;
  const grid = { 2: "grid-cols-2", 3: "grid-cols-3", 4: "grid-cols-3 sm:grid-cols-4" }[columns];
  return (
    <>
      {images.length > 0 && (
        <div className={`grid ${grid} gap-2`}>
          {images.slice(0, cut).map((a, i) => (
            <Thumb
              key={a.id}
              a={a}
              className="aspect-[3/2]"
              label={labels === "caption" ? (a.caption ?? undefined) : undefined}
              onOpen={() => setOpen(i)}
            />
          ))}
          {cut < images.length && (
            <button
              type="button"
              onClick={() => setAll(true)}
              className="border-rule label-mono text-text-quiet hover:text-text aspect-[3/2] cursor-pointer rounded-md border-dashed text-[11px]"
            >
              +{images.length - cut} more
            </button>
          )}
        </div>
      )}
      {files.length > 0 && (
        <div className="flex flex-col gap-2">
          {files.map((a) => (
            <FileRow key={a.id} a={a} onRemove={onRemove ? () => onRemove(a) : undefined} />
          ))}
        </div>
      )}
      {open !== null && (
        <Lightbox items={images} start={open} onClose={() => setOpen(null)} canCover={projectCanCover} />
      )}
    </>
  );
}

/** Wraps an area so files dropped or pasted onto it are attached. */
export function DropArea({
  onFiles,
  disabled,
  children,
  className = "",
  hint,
}: {
  onFiles: (files: File[]) => void;
  disabled?: boolean;
  children: ReactNode;
  className?: string;
  /** Shown over the area while a file is dragged across it. */
  hint: string;
}) {
  const [over, setOver] = useState(0);
  if (disabled) return <div className={className}>{children}</div>;
  const dragging = over > 0;
  return (
    <div
      className={`relative ${className}`}
      onDragEnter={(e) => {
        if (!e.dataTransfer.types.includes("Files")) return;
        e.preventDefault();
        setOver((n) => n + 1);
      }}
      onDragOver={(e) => {
        if (e.dataTransfer.types.includes("Files")) e.preventDefault();
      }}
      onDragLeave={() => setOver((n) => Math.max(0, n - 1))}
      onDrop={(e) => {
        const files = filesFrom(e.dataTransfer);
        setOver(0);
        if (!files.length) return;
        e.preventDefault();
        onFiles(files);
      }}
      onPaste={(e) => {
        const files = filesFrom(e.clipboardData);
        if (!files.length) return;
        e.preventDefault();
        onFiles(files);
      }}
    >
      {children}
      {dragging && (
        <div className="border-accent bg-surface/85 label-mono text-text pointer-events-none absolute inset-0 z-10 grid place-items-center rounded-md border-2 border-dashed text-[12px]">
          {hint}
        </div>
      )}
    </div>
  );
}

/**
 * Photos and files on one thing (a task, a database row): buttons to add,
 * the grid, and a drop box. On phones "Take photo" opens the camera.
 */
export function AttachSection({
  target,
  items,
  name,
  canEdit,
  projectCanCover,
  id,
}: {
  target: Target;
  items: Attachment[];
  /** What they attach to, for the drop box: "Strip interior". */
  name: string;
  canEdit: boolean;
  projectCanCover?: boolean;
  id?: string;
}) {
  const upload = useUpload(target);
  const remove = useDeleteAttachment();
  const add = (files: File[]) => upload.mutate(files);
  if (!canEdit && items.length === 0) return null;
  return (
    <DropArea onFiles={add} disabled={!canEdit} hint={`Drop files to attach to ${name}`}>
      <section id={id} className="flex scroll-mt-4 flex-col gap-2.5" aria-label="Photos and files">
        <div className="flex items-center gap-2">
          <h3 className="label-mono text-text-quiet mr-auto">
            Photos and files {items.length > 0 && <span className="text-accent">· {items.length}</span>}
          </h3>
          {canEdit && (
            <span className="hidden gap-2 sm:flex">
              <PickButton camera onFiles={add} className="min-h-9 px-3 text-sm" disabled={upload.isPending}>
                <Icon d={CAMERA_ICON} /> Photo
              </PickButton>
              <PickButton onFiles={add} className="min-h-9 px-3 text-sm" disabled={upload.isPending}>
                <Icon d={CLIP_ICON} /> Attach
              </PickButton>
            </span>
          )}
        </div>
        {canEdit && (
          <div className="grid grid-cols-2 gap-2 sm:hidden">
            <PickButton camera variant="primary" onFiles={add} className="min-h-12" disabled={upload.isPending}>
              <Icon d={CAMERA_ICON} /> Take photo
            </PickButton>
            <PickButton onFiles={add} className="min-h-12" disabled={upload.isPending}>
              <Icon d={CLIP_ICON} /> Attach file
            </PickButton>
          </div>
        )}
        <FileGrid items={items} max={6} projectCanCover={projectCanCover} onRemove={(a) => remove.mutate(a.id)} />
        {upload.isPending && (
          <p className="label-mono text-text-quiet text-[11px]" role="status">
            Uploading {upload.variables?.length ?? ""}…
          </p>
        )}
        {canEdit && (
          <p className="border-rule label-mono text-text-quiet hidden min-h-14 place-items-center rounded-md border-dashed px-3 text-center text-[11px] sm:grid">
            Drop or paste files to attach to {name}
          </p>
        )}
        <ErrorNote error={upload.error ?? remove.error} />
      </section>
    </DropArea>
  );
}

/** Full-size photos, one at a time, with caption, download and the rest. */
export function Lightbox({
  items,
  start,
  onClose,
  canCover,
}: {
  items: Attachment[];
  start: number;
  onClose: () => void;
  /** Offer "Use as cover on the canvas". */
  canCover?: boolean;
}) {
  const [index, setIndex] = useState(start);
  const a = items[Math.min(index, items.length - 1)];
  const update = useUpdateAttachment();
  const remove = useDeleteAttachment();
  const cover = useSetCover();
  const [caption, setCaption] = useState(a?.caption ?? "");
  const [confirming, setConfirming] = useState(false);
  useEffect(() => {
    setCaption(a?.caption ?? "");
    setConfirming(false);
  }, [a?.id, a?.caption]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement) {
        if (e.key === "Escape") e.target.blur();
        return;
      }
      if (e.key === "Escape") (e.stopPropagation(), onClose());
      if (e.key === "ArrowRight") setIndex((i) => Math.min(items.length - 1, i + 1));
      if (e.key === "ArrowLeft") setIndex((i) => Math.max(0, i - 1));
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [items.length, onClose]);
  useEffect(() => {
    if (items.length === 0) onClose();
  }, [items.length, onClose]);
  if (!a) return null;

  const saveCaption = () => caption.trim() !== (a.caption ?? "") && update.mutate({ id: a.id, caption: caption.trim() });
  return (
    <div role="dialog" aria-modal="true" aria-label={a.caption ?? a.name} className="fixed inset-0 z-[60] flex flex-col bg-[rgba(0,0,0,0.96)] text-[#fff]">
      <div className="flex items-center gap-3 px-4 py-3">
        <span className="label-mono min-w-0 flex-1 truncate text-[11px] text-[rgba(255,255,255,0.70)]">
          {index + 1} / {items.length} · {a.taken_at ? "taken" : "added"} {shortDay(when(a))}
          {a.added_by_name && ` · ${a.added_by_name}`}
          <ViaBadge via={a.via} />
        </span>
        <button type="button" onClick={onClose} aria-label="Close" className="grid size-11 cursor-pointer place-items-center text-2xl">
          ×
        </button>
      </div>
      <div className="relative flex min-h-0 flex-1 items-center justify-center px-2">
        <img key={a.id} src={fileUrl(a, 1600)} alt={a.caption ?? a.name} className="max-h-full max-w-full object-contain" />
        {index > 0 && (
          <button type="button" aria-label="Previous" onClick={() => setIndex(index - 1)} className="absolute top-1/2 left-2 grid size-12 -translate-y-1/2 cursor-pointer place-items-center rounded-full bg-[rgba(0,0,0,0.50)] text-2xl">
            ‹
          </button>
        )}
        {index < items.length - 1 && (
          <button type="button" aria-label="Next" onClick={() => setIndex(index + 1)} className="absolute top-1/2 right-2 grid size-12 -translate-y-1/2 cursor-pointer place-items-center rounded-full bg-[rgba(0,0,0,0.50)] text-2xl">
            ›
          </button>
        )}
      </div>
      <div className="flex flex-col gap-2 px-4 pt-3 pb-[max(1rem,env(safe-area-inset-bottom))]">
        {a.can_edit ? (
          <input
            aria-label="Caption"
            className="font-reading w-full border-0 border-b border-[rgba(255,255,255,0.25)] bg-transparent py-1.5 text-[17px] text-[#fff] outline-none placeholder:text-[rgba(255,255,255,0.40)] focus:border-[rgba(255,255,255,0.70)]"
            placeholder="Add a caption…"
            value={caption}
            maxLength={300}
            onChange={(e) => setCaption(e.target.value)}
            onBlur={saveCaption}
            onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          />
        ) : (
          a.caption && <p className="font-reading text-[17px]">{a.caption}</p>
        )}
        <div className="label-mono flex flex-wrap gap-x-4 gap-y-1 text-[11px] text-[rgba(255,255,255,0.75)]">
          {a.task_id && a.project_id && (
            <Link to={`/projects/${a.project_id}/board?task=${a.task_id}`} onClick={onClose} className="min-h-9 content-center hover:text-[#fff]">
              Go to T-{a.task_number} {a.task_title}
            </Link>
          )}
          {a.note_id && (
            <Link to={`/notes/${a.note_id}`} onClick={onClose} className="min-h-9 content-center hover:text-[#fff]">
              Open note {a.note_title}
            </Link>
          )}
          <a href={fileUrl(a, undefined, true)} className="min-h-9 content-center hover:text-[#fff]">
            Download original · {fileSize(a.size)}
          </a>
          {canCover && a.project_id && a.can_edit && (
            <button
              type="button"
              className="min-h-9 cursor-pointer hover:text-[#fff]"
              onClick={() => cover.mutate({ projectId: a.project_id!, attachmentId: a.id })}
            >
              {cover.isSuccess && cover.variables?.attachmentId === a.id ? "✓ Cover on the canvas" : "Use as cover on the canvas"}
            </button>
          )}
          {a.can_edit &&
            (confirming ? (
              <>
                <button type="button" className="min-h-9 cursor-pointer text-[#ff9d8a]" onClick={() => remove.mutate(a.id)}>
                  Delete for good
                </button>
                <button type="button" className="min-h-9 cursor-pointer hover:text-[#fff]" onClick={() => setConfirming(false)}>
                  Keep it
                </button>
              </>
            ) : (
              <button type="button" className="min-h-9 cursor-pointer hover:text-[#ff9d8a]" onClick={() => setConfirming(true)}>
                Delete…
              </button>
            ))}
        </div>
        <ErrorNote error={update.error ?? remove.error ?? cover.error} />
      </div>
    </div>
  );
}

/** "T-5 Strip interior · 9 Nov" on a Photos tab tile. */
export function datedLabel(a: Attachment): string {
  return `${tileLabel(a)} · ${shortDay(when(a))}`;
}
