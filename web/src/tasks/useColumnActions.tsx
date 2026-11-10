import { useEffect, useRef, useState } from "react";
import type { Status } from "../api/client";
import { ConfirmDone } from "./ColumnParts";
import { placeText } from "./columns";
import { useMoveStatus, useUpdateStatus } from "./queries";

interface Toast {
  text: string;
  undo?: () => void;
}

/**
 * The column changes the board and the Columns panel share: moves (with an
 * Undo toast) and category changes (asking first before a column full of
 * tasks starts counting as done). Render `ui` once.
 */
export function useColumnActions(projectId: string, statuses: Status[], counts: Record<string, number>) {
  const update = useUpdateStatus(projectId);
  const moveStatus = useMoveStatus(projectId);
  const [confirm, setConfirm] = useState<Status | null>(null);
  const [toast, setToast] = useState<Toast | null>(null);
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => () => window.clearTimeout(timer.current), []);
  const show = (t: Toast) => {
    window.clearTimeout(timer.current);
    setToast(t);
    timer.current = window.setTimeout(() => setToast(null), 6000);
  };

  const move = (status: Status, afterId: string | null, quiet = false) => {
    const i = statuses.findIndex((s) => s.id === status.id);
    const was = i > 0 ? statuses[i - 1]!.id : null;
    if (was === afterId) return;
    const others = statuses.filter((s) => s.id !== status.id);
    moveStatus.mutate({ id: status.id, afterId });
    if (quiet) return;
    show({
      text: `Moved ${status.name} ${placeText(others, afterId)}`,
      undo: () => {
        moveStatus.mutate({ id: status.id, afterId: was });
        setToast(null);
      },
    });
  };

  const setCategory = (status: Status, category: string) => {
    if (category === status.category) return;
    if (category === "done" && (counts[status.id] ?? 0) > 0) setConfirm(status);
    else update.mutate({ id: status.id, category });
  };

  const rename = (status: Status, name: string) => {
    const n = name.trim();
    if (n && n !== status.name) update.mutate({ id: status.id, name: n });
  };

  const ui = (
    <>
      {confirm && (
        <ConfirmDone
          status={confirm}
          count={counts[confirm.id] ?? 0}
          onCancel={() => setConfirm(null)}
          onConfirm={() => {
            update.mutate({ id: confirm.id, category: "done" });
            setConfirm(null);
          }}
        />
      )}
      {toast && (
        <div
          role="status"
          className="border-rule bg-surface-raised shadow-card fixed bottom-20 left-1/2 z-50 flex -translate-x-1/2 items-center gap-4 rounded-md px-4 py-2.5 sm:bottom-6"
        >
          <span className="font-semibold">{toast.text}</span>
          {toast.undo && (
            <button type="button" onClick={toast.undo} className="label-mono text-accent min-h-9 cursor-pointer px-1 text-[11px]">
              Undo
            </button>
          )}
        </div>
      )}
    </>
  );

  return { move, setCategory, rename, error: update.error ?? moveStatus.error, ui };
}
