import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, ApiError, unwrap } from "../api/client";
import type { components } from "../api/schema";
import type { NoteDetail } from "./format";

type CreateNote = components["schemas"]["CreateNoteRequest"];
type UpdateNote = components["schemas"]["UpdateNoteRequest"];
export type ImportResult = components["schemas"]["ImportResult"];

export interface NoteFilter {
  q?: string;
  tag?: string;
  folder?: string;
  project_id?: string;
  limit?: number;
}

export const noteKeys = {
  all: ["notes"] as const,
  list: (filter: NoteFilter) => ["notes", "list", filter] as const,
  tags: ["notes", "tags"] as const,
  note: (id: string) => ["notes", "one", id] as const,
  project: (id: string) => ["notes", "project", id] as const,
};

export function useNotes(filter: NoteFilter = {}) {
  return useQuery({
    queryKey: noteKeys.list(filter),
    queryFn: () => unwrap(api.GET("/notes", { params: { query: filter } })),
    placeholderData: (prev) => prev,
  });
}

export function useNoteTags() {
  return useQuery({ queryKey: noteKeys.tags, queryFn: () => unwrap(api.GET("/notes/tags")) });
}

export function useNote(id: string | undefined) {
  return useQuery({
    queryKey: noteKeys.note(id ?? ""),
    queryFn: () => unwrap(api.GET("/notes/{id}", { params: { path: { id: id! } } })),
    enabled: !!id,
  });
}

export function useProjectNotes(projectId: string) {
  return useQuery({
    queryKey: noteKeys.project(projectId),
    queryFn: () => unwrap(api.GET("/projects/{id}/notes", { params: { path: { id: projectId } } })),
  });
}

export function useCreateNote() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateNote) => unwrap(api.POST("/notes", { body })),
    onSuccess: (note) => {
      qc.setQueryData(noteKeys.note(note.note.id), note);
      qc.invalidateQueries({ queryKey: noteKeys.all, predicate: (q) => q.queryKey[1] !== "one" });
    },
  });
}

export function useUpdateNote(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: UpdateNote) => unwrap(api.PATCH("/notes/{id}", { params: { path: { id } }, body })),
    onSuccess: (note: NoteDetail, sent) => {
      qc.setQueryData(noteKeys.note(id), note);
      qc.invalidateQueries({
        queryKey: noteKeys.all,
        // Lists always; other open notes only after a rename, which rewrites their links.
        predicate: (q) => q.queryKey[1] !== "one" || (sent.title !== undefined && q.queryKey[2] !== id),
      });
    },
  });
}

export function useDeleteNote() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE("/notes/{id}", { params: { path: { id } } })),
    onSuccess: (_, id) => {
      qc.removeQueries({ queryKey: noteKeys.note(id) });
      qc.invalidateQueries({ queryKey: noteKeys.all });
    },
  });
}

export function useRescan() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(api.POST("/notes/rescan")),
    onSuccess: () => qc.invalidateQueries({ queryKey: noteKeys.all }),
  });
}

/** Sends a zip as the raw request body; openapi-fetch would JSON-encode it. */
export function useImportNotes() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ file, source, folder }: { file: File; source: "obsidian" | "notion"; folder: string }) => {
      const params = new URLSearchParams({ source });
      if (folder.trim()) params.set("folder", folder.trim());
      const res = await fetch(`/api/v1/notes/import?${params}`, {
        method: "POST",
        credentials: "same-origin",
        headers: { "Content-Type": "application/zip" },
        body: file,
      });
      const body = await res.json().catch(() => ({}));
      if (!res.ok) throw new ApiError(res.status, body.code ?? "unknown", body.message ?? `The server answered ${res.status}.`);
      return body as ImportResult;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: noteKeys.all }),
  });
}
