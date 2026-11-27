import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { api, unwrap } from "../api/client";
import type { components } from "../api/schema";
import { BoardDoc } from "./doc";

export type Whiteboard = components["schemas"]["Whiteboard"];
export type Cursor = components["schemas"]["Cursor"];

export const boardKeys = {
  project: (projectId: string) => ["whiteboards", projectId] as const,
  elements: (id: string) => ["whiteboard-elements", id] as const,
};

export function useWhiteboards(projectId: string) {
  return useQuery({
    queryKey: boardKeys.project(projectId),
    queryFn: () => unwrap(api.GET("/projects/{id}/whiteboards", { params: { path: { id: projectId } } })),
    refetchInterval: 30_000,
  });
}

/** A board's elements once, for the little previews in the list. */
export function useBoardPreview(id: string, seq: number) {
  return useQuery({
    queryKey: [...boardKeys.elements(id), seq],
    queryFn: () => unwrap(api.GET("/whiteboards/{id}/elements", { params: { path: { id } } })),
    staleTime: Infinity,
  });
}

export function useCreateWhiteboard(projectId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (title: string) =>
      unwrap(api.POST("/projects/{id}/whiteboards", { params: { path: { id: projectId } }, body: { title } })),
    onSuccess: () => qc.invalidateQueries({ queryKey: boardKeys.project(projectId) }),
  });
}

export function useUpdateWhiteboard(projectId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, ...body }: { id: string; title?: string; after?: string | null }) =>
      unwrap(api.PATCH("/whiteboards/{id}", { params: { path: { id } }, body })),
    onSuccess: () => qc.invalidateQueries({ queryKey: boardKeys.project(projectId) }),
  });
}

export function useDeleteWhiteboard(projectId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE("/whiteboards/{id}", { params: { path: { id } } })),
    onSuccess: () => qc.invalidateQueries({ queryKey: boardKeys.project(projectId) }),
  });
}

/** How long to wait before saving after a change, and between looks for other people's. */
const SAVE_AFTER = 400;
const POLL_ALONE = 2500;
const POLL_TOGETHER = 900;

/**
 * Keeps an open board in step with the server: loads it, saves changes a
 * moment after you make them, and takes in other people's every second or
 * two. Returns the board document, who else is looking, and a way to share
 * your pointer.
 */
export function useBoardSync(boardId: string, canEdit: boolean) {
  const [doc] = useState(() => new BoardDoc());
  const [cursors, setCursors] = useState<Cursor[]>([]);
  const [error, setError] = useState<string | null>(null);
  const rev = useSyncExternalStore(doc.subscribe, doc.getRev);
  const together = useRef(false);
  const saving = useRef<Promise<void> | null>(null);
  const qc = useQueryClient();

  // Load, then poll.
  useEffect(() => {
    let stop = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const poll = async () => {
      if (stop) return;
      if (document.visibilityState === "visible") {
        try {
          const res = await unwrap(
            api.GET("/whiteboards/{id}/elements", {
              params: { path: { id: boardId }, query: doc.loaded ? { since: doc.seq } : {} },
            }),
          );
          if (stop) return;
          if (doc.loaded) doc.applyRemote(res.elements, res.seq);
          else doc.load(res.elements, res.seq);
          setCursors(res.cursors);
          together.current = res.cursors.length > 0;
          setError(null);
        } catch (e) {
          if (!doc.loaded) setError(e instanceof Error ? e.message : "Couldn't open this whiteboard.");
        }
      }
      timer = setTimeout(poll, together.current ? POLL_TOGETHER : POLL_ALONE);
    };
    poll();
    return () => {
      stop = true;
      clearTimeout(timer);
    };
  }, [boardId, doc]);

  // Save a moment after each change; one save at a time.
  useEffect(() => {
    if (!canEdit || doc.state !== "unsaved" || saving.current) return;
    const t = setTimeout(() => {
      const send = async () => {
        const changes = doc.takeChanges();
        if (!changes.length) return;
        try {
          const res = await unwrap(
            api.PATCH("/whiteboards/{id}/elements", { params: { path: { id: boardId } }, body: { elements: changes } }),
          );
          doc.saved(res.rejected);
        } catch (e) {
          doc.failed(!navigator.onLine);
          setError(e instanceof Error ? e.message : "Couldn't save.");
          // Try again in a while rather than at once.
          await new Promise((r) => setTimeout(r, 3000));
        }
      };
      saving.current = send().finally(() => {
        saving.current = null;
        // Wake the effect in case more changes came in meanwhile.
        if (doc.state === "unsaved") doc.live({});
      });
    }, SAVE_AFTER);
    return () => clearTimeout(t);
  }, [rev, canEdit, boardId, doc]);

  // Save what's left when leaving, even if the page is closing.
  useEffect(() => {
    const flush = () => {
      if (!canEdit || !doc.pending) return;
      const changes = doc.takeChanges();
      if (!changes.length) return;
      fetch(`/api/v1/whiteboards/${boardId}/elements`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ elements: changes }),
        credentials: "same-origin",
        keepalive: true,
      }).catch(() => {});
    };
    const warn = (e: BeforeUnloadEvent) => {
      if (doc.pending) {
        flush();
        e.preventDefault();
      }
    };
    window.addEventListener("beforeunload", warn);
    return () => {
      window.removeEventListener("beforeunload", warn);
      flush();
      qc.invalidateQueries({ queryKey: ["whiteboards"] });
    };
  }, [boardId, canEdit, doc, qc]);

  // Your pointer, at most a few times a second.
  const last = useRef(0);
  const queued = useRef<[number, number] | null>(null);
  const shareCursor = (x: number, y: number) => {
    queued.current = [x, y];
    const now = Date.now();
    const wait = Math.max(0, 250 - (now - last.current));
    if (wait > 0 && last.current) {
      if (!pending.current) pending.current = setTimeout(sendCursor, wait);
      return;
    }
    sendCursor();
  };
  const pending = useRef<ReturnType<typeof setTimeout> | null>(null);
  const sendCursor = () => {
    pending.current = null;
    const p = queued.current;
    if (!p) return;
    last.current = Date.now();
    api
      .PUT("/whiteboards/{id}/cursor", { params: { path: { id: boardId } }, body: { x: p[0], y: p[1] } })
      .catch(() => {});
  };

  return { doc, rev, cursors, error, shareCursor };
}
