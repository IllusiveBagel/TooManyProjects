// A project's whiteboards: tabs and the open board on desktop; on a phone, a
// list of boards that each open full screen.
import { useEffect, useMemo, useRef, useState, useSyncExternalStore, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useNavigate, useParams } from "react-router";
import { Modal } from "../components/Modal";
import { Button, ErrorNote, ViaBadge } from "../components/ui";
import { ago } from "../notes/format";
import { useOpenTask } from "../tasks/TaskDrawer";
import type { BoardDoc, SaveState } from "../whiteboards/doc";
import { Editor } from "../whiteboards/Editor";
import { downloadPng, downloadSvg } from "../whiteboards/export";
import { fromData, resolvedFonts, sortByZ, type El } from "../whiteboards/model";
import {
  useBoardPreview,
  useBoardSync,
  useCreateWhiteboard,
  useDeleteWhiteboard,
  useUpdateWhiteboard,
  useWhiteboards,
  type Whiteboard,
} from "../whiteboards/queries";
import { Preview } from "../whiteboards/Scene";
import { useProject } from "./ProjectPage";

const PHONE = "(max-width: 639px)";
const subscribe = (f: () => void) => {
  const m = window.matchMedia(PHONE);
  m.addEventListener("change", f);
  return () => m.removeEventListener("change", f);
};
const usePhone = () => useSyncExternalStore(subscribe, () => window.matchMedia(PHONE).matches);

const tab = (on: boolean) =>
  `label-mono inline-flex min-h-9 shrink-0 cursor-pointer items-center gap-1.5 rounded-md px-2.5 text-[11px] ${on ? "bg-accent text-on-accent" : "text-text-quiet hover:text-text hover:bg-surface-raised"}`;

const SAVE_TEXT: Record<SaveState, string> = {
  saved: "Saved",
  saving: "Saving…",
  unsaved: "Saving…",
  offline: "Offline · will save when back",
};

/** A name for a new board that isn't taken yet. */
function nextTitle(boards: Whiteboard[]) {
  const taken = new Set(boards.map((b) => b.title.toLowerCase()));
  for (let n = boards.length + 1; ; n++) if (!taken.has(`whiteboard ${n}`)) return `Whiteboard ${n}`;
}

export function ProjectWhiteboardsPage() {
  const { detail, canEdit } = useProject();
  const projectId = detail.project.id;
  const { boardId } = useParams();
  const navigate = useNavigate();
  const phone = usePhone();
  const boards = useWhiteboards(projectId);
  const create = useCreateWhiteboard(projectId);
  const [renaming, setRenaming] = useState<string | null>(null);
  const list = boards.data ?? [];
  const current = list.find((b) => b.id === boardId);
  const open = (id: string, replace = false) => navigate(`/projects/${projectId}/whiteboards/${id}`, { replace });

  // Desktop always shows a board: the first, if none is picked.
  useEffect(() => {
    if (!phone && !boardId && list[0]) open(list[0].id, true);
    if (boardId && boards.data && !current) navigate(`/projects/${projectId}/whiteboards`, { replace: true });
  }, [phone, boardId, list, current, boards.data]); // eslint-disable-line react-hooks/exhaustive-deps

  const newBoard = () =>
    create.mutate(nextTitle(list), {
      onSuccess: (b) => {
        open(b.id);
        if (!phone) setRenaming(b.id);
      },
    });

  if (boards.isError) return <div className="p-5"><ErrorNote error={boards.error} /></div>;
  if (!boards.data) return <p className="label-mono text-text-quiet p-5">Loading…</p>;

  if (phone) {
    return (
      <>
        <BoardList boards={list} canEdit={canEdit} onOpen={(id) => open(id)} onNew={newBoard} creating={create.isPending} />
        {create.isError && <div className="px-4"><ErrorNote error={create.error} /></div>}
        {current &&
          createPortal(
            <PhoneBoard
              key={current.id}
              board={current}
              projectId={projectId}
              canEdit={canEdit}
              onBack={() => navigate(`/projects/${projectId}/whiteboards`)}
            />,
            document.body,
          )}
      </>
    );
  }

  if (!list.length) {
    return (
      <div className="flex flex-col items-center gap-4 px-5 py-16 text-center">
        <p className="font-reading text-text-quiet max-w-md text-[18px]">
          Whiteboards are for working things out: wiring plans, episode outlines, layouts. Boxes, arrows, sticky notes and a pen.
        </p>
        {canEdit && (
          <Button variant="primary" onClick={newBoard} disabled={create.isPending}>
            + New whiteboard
          </Button>
        )}
        {create.isError && <ErrorNote error={create.error} />}
      </div>
    );
  }

  return (
    <div className="flex min-h-[560px] flex-1 flex-col">
      {current && (
        <DesktopBoard
          key={current.id}
          board={current}
          boards={list}
          projectId={projectId}
          canEdit={canEdit}
          renaming={renaming === current.id}
          setRenaming={(on) => setRenaming(on ? current.id : null)}
          onOpen={open}
          onNew={newBoard}
          creating={create.isPending}
        />
      )}
    </div>
  );
}

function DesktopBoard({
  board,
  boards,
  projectId,
  canEdit,
  renaming,
  setRenaming,
  onOpen,
  onNew,
  creating,
}: {
  board: Whiteboard;
  boards: Whiteboard[];
  projectId: string;
  canEdit: boolean;
  renaming: boolean;
  setRenaming: (on: boolean) => void;
  onOpen: (id: string) => void;
  onNew: () => void;
  creating: boolean;
}) {
  const sync = useBoardSync(board.id, canEdit);
  const [, openTask] = useOpenTask();
  const [menu, setMenu] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const update = useUpdateWhiteboard(projectId);
  return (
    <>
      <div className="border-rule flex items-center gap-1 overflow-x-auto border-x-0 border-t-0 px-4 py-1.5 sm:px-6">
        {boards.map((b) =>
          b.id === board.id && renaming ? (
            <RenameInput
              key={b.id}
              title={b.title}
              onDone={(title) => {
                setRenaming(false);
                if (title && title !== b.title) update.mutate({ id: b.id, title });
              }}
            />
          ) : (
            <button
              key={b.id}
              type="button"
              className={tab(b.id === board.id)}
              aria-current={b.id === board.id ? "page" : undefined}
              onClick={() => onOpen(b.id)}
              onDoubleClick={() => canEdit && b.id === board.id && setRenaming(true)}
              title={canEdit && b.id === board.id ? "Double-click to rename" : undefined}
            >
              {b.title}
            </button>
          ),
        )}
        {canEdit && (
          <button type="button" className={tab(false)} onClick={onNew} disabled={creating}>
            + New whiteboard
          </button>
        )}
        <span className="flex-1" />
        <SaveStatus doc={sync.doc} board={board} error={sync.error} />
        <div className="relative shrink-0">
          <button type="button" className={tab(menu)} aria-haspopup="menu" aria-expanded={menu} onClick={() => setMenu((m) => !m)}>
            ⋯ More
          </button>
          {menu && (
            <BoardMenu
              canEdit={canEdit}
              onClose={() => setMenu(false)}
              onRename={() => setRenaming(true)}
              onDelete={() => setDeleting(true)}
              onExport={(kind) => (kind === "png" ? downloadPng : downloadSvg)(board.title, sync.doc.list())}
            />
          )}
        </div>
      </div>
      {update.isError && <div className="px-6 pt-2"><ErrorNote error={update.error} /></div>}
      {sync.error && !sync.doc.loaded ? (
        <div className="p-5"><ErrorNote error={new Error(sync.error)} /></div>
      ) : (
        <Editor doc={sync.doc} rev={sync.rev} canEdit={canEdit} projectId={projectId} cursors={sync.cursors} shareCursor={sync.shareCursor} onOpenTask={openTask} />
      )}
      {deleting && <DeleteDialog board={board} projectId={projectId} onClose={() => setDeleting(false)} />}
    </>
  );
}

function PhoneBoard({ board, projectId, canEdit, onBack }: { board: Whiteboard; projectId: string; canEdit: boolean; onBack: () => void }) {
  const sync = useBoardSync(board.id, canEdit);
  const navigate = useNavigate();
  const { doc } = sync;
  // The page under the board shouldn't scroll while it's open.
  useEffect(() => {
    const was = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = was;
    };
  }, []);
  return (
    <div className="bg-surface text-text fixed inset-0 z-50 flex flex-col" role="dialog" aria-label={board.title}>
      <div className="border-rule flex min-h-13 items-center gap-1 border-x-0 border-t-0 px-1 pt-[env(safe-area-inset-top)]">
        <button type="button" aria-label="Back to whiteboards" className="text-text-quiet min-h-11 min-w-11 cursor-pointer text-[18px]" onClick={onBack}>
          ←
        </button>
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="font-display truncate text-[17px] leading-tight font-extrabold">{board.title}</span>
          <span className="label-mono text-text-quiet text-[9px]">{canEdit ? SAVE_TEXT[doc.state] : "View only"}</span>
        </span>
        {canEdit && (
          <>
            <IconButton label="Undo" disabled={!doc.canUndo} onClick={() => doc.undo()}>↶</IconButton>
            <IconButton label="Redo" disabled={!doc.canRedo} onClick={() => doc.redo()}>↷</IconButton>
          </>
        )}
      </div>
      <div className="relative flex min-h-0 flex-1 flex-col">
        {sync.error && !doc.loaded ? (
          <div className="p-4"><ErrorNote error={new Error(sync.error)} /></div>
        ) : (
          <Editor doc={doc} rev={sync.rev} canEdit={canEdit} projectId={projectId} cursors={sync.cursors} shareCursor={sync.shareCursor} phone onOpenTask={(id) => navigate(`/projects/${projectId}/whiteboards?task=${id}`)} />
        )}
      </div>
    </div>
  );
}

function SaveStatus({ doc, board, error }: { doc: BoardDoc; board: Whiteboard; error: string | null }) {
  useSyncExternalStore(doc.subscribe, doc.getRev);
  const text =
    doc.state !== "saved"
      ? SAVE_TEXT[doc.state]
      : error && doc.loaded
        ? "Not saved · retrying"
        : board.updated_by_name
          ? `Saved · edited ${ago(board.updated_at)} by ${board.updated_by_name}`
          : "Saved";
  return (
    <span className="label-mono text-text-quiet shrink-0 px-2 text-[10px]" role="status">
      {text}
      {doc.state === "saved" && board.updated_via === "ai" && <ViaBadge via="ai" />}
    </span>
  );
}

function RenameInput({ title, onDone }: { title: string; onDone: (title: string | null) => void }) {
  const [value, setValue] = useState(title);
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);
  return (
    <input
      ref={ref}
      value={value}
      maxLength={80}
      aria-label="Whiteboard name"
      onChange={(e) => setValue(e.target.value)}
      onBlur={() => onDone(value.trim() || null)}
      onKeyDown={(e) => {
        if (e.key === "Enter") onDone(value.trim() || null);
        if (e.key === "Escape") onDone(null);
      }}
      className="label-mono border-accent bg-surface text-text min-h-9 w-48 shrink-0 rounded-md border-2 px-2 text-[11px] outline-none"
    />
  );
}

function BoardMenu({
  canEdit,
  onClose,
  onRename,
  onDelete,
  onExport,
}: {
  canEdit: boolean;
  onClose: () => void;
  onRename: () => void;
  onDelete: () => void;
  onExport: (kind: "png" | "svg") => Promise<void>;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    ref.current?.querySelector<HTMLElement>("[role=menuitem]")?.focus();
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    const t = setTimeout(() => document.addEventListener("mousedown", onDown));
    window.addEventListener("keydown", onKey);
    return () => {
      clearTimeout(t);
      document.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);
  const item = "flex min-h-9 w-full cursor-pointer items-center rounded-sm px-2.5 text-left text-sm font-semibold hover:bg-line focus:bg-line focus:outline-none disabled:opacity-40";
  const run = (f: () => void | Promise<void>) => async () => {
    setBusy(true);
    try {
      await f();
    } finally {
      setBusy(false);
      onClose();
    }
  };
  return (
    <div ref={ref} role="menu" aria-label="Whiteboard" className="border-rule bg-surface-raised shadow-card absolute top-10 right-0 z-30 flex w-56 flex-col rounded-lg p-1.5">
      {canEdit && (
        <button type="button" role="menuitem" className={item} onClick={run(onRename)}>
          Rename
        </button>
      )}
      <button type="button" role="menuitem" className={item} disabled={busy} onClick={run(() => onExport("png"))}>
        Download PNG
      </button>
      <button type="button" role="menuitem" className={item} disabled={busy} onClick={run(() => onExport("svg"))}>
        Download SVG
      </button>
      {canEdit && (
        <>
          <div className="bg-line mx-1 my-1 h-[1.5px]" role="separator" />
          <button type="button" role="menuitem" className={`${item} text-late`} onClick={run(onDelete)}>
            Delete whiteboard…
          </button>
        </>
      )}
    </div>
  );
}

function DeleteDialog({ board, projectId, onClose }: { board: Whiteboard; projectId: string; onClose: () => void }) {
  const remove = useDeleteWhiteboard(projectId);
  return (
    <Modal title={`Delete ${board.title}?`} onClose={onClose}>
      <p className="font-reading text-[16px]">
        Everything drawn on it goes for everyone. Photos stay in the project&rsquo;s Photos.
      </p>
      {remove.isError && <ErrorNote error={remove.error} />}
      <div className="mt-5 flex justify-end gap-2">
        <Button onClick={onClose}>Keep it</Button>
        <Button variant="primary" disabled={remove.isPending} onClick={() => remove.mutate(board.id, { onSuccess: onClose })}>
          Delete
        </Button>
      </div>
    </Modal>
  );
}

function IconButton({ label, children, onClick, disabled }: { label: string; children: ReactNode; onClick?: () => void; disabled?: boolean }) {
  return (
    <button type="button" aria-label={label} title={label} disabled={disabled} onClick={onClick} className="text-text hover:bg-surface-raised grid min-h-11 min-w-11 cursor-pointer place-items-center rounded-md text-[18px] disabled:opacity-35">
      {children}
    </button>
  );
}

function BoardList({ boards, canEdit, onOpen, onNew, creating }: { boards: Whiteboard[]; canEdit: boolean; onOpen: (id: string) => void; onNew: () => void; creating: boolean }) {
  return (
    <div className="flex flex-col gap-3 p-4">
      {!boards.length && (
        <p className="font-reading text-text-quiet text-[17px]">
          No whiteboards yet. They&rsquo;re for working things out: wiring plans, outlines, layouts.
        </p>
      )}
      {boards.map((b) => (
        <button key={b.id} type="button" onClick={() => onOpen(b.id)} className="border-rule bg-surface-card shadow-card flex cursor-pointer flex-col overflow-hidden rounded-lg text-left">
          <span className="bg-surface border-rule block h-40 border-x-0 border-t-0">
            <BoardThumb board={b} />
          </span>
          <span className="flex flex-col gap-0.5 px-3 py-2.5">
            <span className="font-display text-[17px] font-extrabold">{b.title}</span>
            <span className="label-mono text-text-quiet text-[10px]">
              Edited {ago(b.updated_at)}
              {b.updated_by_name && ` · ${b.updated_by_name}`}
              <ViaBadge via={b.updated_via} />
            </span>
          </span>
        </button>
      ))}
      {canEdit && (
        <button type="button" onClick={onNew} disabled={creating} className="border-rule label-mono text-text-quiet min-h-12 cursor-pointer rounded-lg border-dashed text-[11px]">
          + New whiteboard
        </button>
      )}
    </div>
  );
}

function BoardThumb({ board }: { board: Whiteboard }) {
  const q = useBoardPreview(board.id, board.seq);
  const fonts = useMemo(() => resolvedFonts(), []);
  const els = useMemo(
    () => sortByZ((q.data?.elements ?? []).map((s) => fromData(s.id, s.kind, s.data as Record<string, unknown>)).filter((e): e is El => !!e)),
    [q.data],
  );
  if (!els.length) return <span className="label-mono text-text-quiet grid h-full place-items-center text-[10px]">{q.data ? "Empty" : ""}</span>;
  return <Preview els={els} measure={fonts} />;
}
