import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { api, ApiError, unwrap } from "../api/client";
import type { Attachment } from "./format";

/** What files are attached to: exactly one of these. */
export type Target =
  | { project_id: string }
  | { task_id: string }
  | { comment_id: string }
  | { row_id: string }
  | { note_id: string };

export interface ListFilter {
  project_id?: string;
  include_children?: boolean;
  task_id?: string;
  row_id?: string;
  note_id?: string;
  images_only?: boolean;
}

export const fileKeys = {
  list: (filter: ListFilter) => ["attachments", filter] as const,
};

export function useAttachments(filter: ListFilter | null) {
  return useQuery({
    queryKey: fileKeys.list(filter ?? {}),
    queryFn: () => unwrap(api.GET("/attachments", { params: { query: filter! } })),
    enabled: !!filter,
  });
}

/** Everything that shows files or their counts. */
function refreshFiles(qc: QueryClient) {
  for (const key of [["attachments"], ["task"], ["tasks"], ["rows"], ["task-rows"], ["canvas"], ["dashboard"]]) {
    qc.invalidateQueries({ queryKey: key });
  }
}

/** Sends files one request each, so one bad file doesn't sink the rest. */
export async function uploadFiles(target: Target, files: File[]): Promise<Attachment[]> {
  const query = new URLSearchParams(target as Record<string, string>).toString();
  const added: Attachment[] = [];
  for (const file of files) {
    const form = new FormData();
    form.append("file", file, file.name || "photo.jpg");
    const res = await fetch(`/api/v1/attachments?${query}`, { method: "POST", body: form, credentials: "same-origin" });
    if (!res.ok) {
      const body = (await res.json().catch(() => ({}))) as { code?: string; message?: string };
      const message = res.status === 413 ? "That file is too large." : (body.message ?? `The server answered ${res.status}.`);
      throw new ApiError(res.status, body.code ?? "unknown", `${file.name}: ${message}`);
    }
    added.push(...((await res.json()) as Attachment[]));
  }
  return added;
}

export function useUpload(target: Target | null) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (files: File[]) => uploadFiles(target!, files),
    onSettled: () => refreshFiles(qc),
  });
}

export function useUpdateAttachment() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, caption }: { id: string; caption: string }) =>
      unwrap(api.PATCH("/attachments/{id}", { params: { path: { id } }, body: { caption } })),
    onSuccess: () => refreshFiles(qc),
  });
}

export function useDeleteAttachment() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE("/attachments/{id}", { params: { path: { id } } })),
    onSuccess: () => refreshFiles(qc),
  });
}

export function useSetCover() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ projectId, attachmentId }: { projectId: string; attachmentId: string | null }) =>
      unwrap(api.PUT("/projects/{id}/cover", { params: { path: { id: projectId } }, body: { attachment_id: attachmentId } })),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["canvas"] });
      qc.invalidateQueries({ queryKey: ["project"] });
    },
  });
}
