import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, unwrap } from "../api/client";
import type { components } from "../api/schema";
import type { Row } from "./format";

type CreateColumn = components["schemas"]["CreateColumnRequest"];
type UpdateColumn = components["schemas"]["UpdateColumnRequest"];
type CreateView = components["schemas"]["CreateViewRequest"];
type UpdateView = components["schemas"]["UpdateViewRequest"];
type CreateRow = components["schemas"]["CreateRowRequest"];
type UpdateRow = components["schemas"]["UpdateRowRequest"];

export const dataKeys = {
  project: (projectId: string) => ["databases", "project", projectId] as const,
  all: ["databases", "all"] as const,
  one: (id: string) => ["database", id] as const,
  rows: (id: string, view?: string | null, q?: string) => ["rows", id, view ?? "", q ?? ""] as const,
  task: (taskId: string) => ["task-rows", taskId] as const,
};

/** A project's databases with their columns and views. */
export function useProjectDatabases(projectId: string | undefined) {
  return useQuery({
    queryKey: dataKeys.project(projectId ?? ""),
    queryFn: () => unwrap(api.GET("/projects/{id}/databases", { params: { path: { id: projectId! } } })),
    enabled: !!projectId,
  });
}

/** Every database you can see, for embeds and pickers. */
export function useAllDatabases(enabled = true) {
  return useQuery({ queryKey: dataKeys.all, queryFn: () => unwrap(api.GET("/databases")), enabled, staleTime: 30_000 });
}

export function useDatabase(id: string | undefined) {
  return useQuery({
    queryKey: dataKeys.one(id ?? ""),
    queryFn: () => unwrap(api.GET("/databases/{id}", { params: { path: { id: id! } } })),
    enabled: !!id,
  });
}

export function useRows(id: string | undefined, view?: string | null, q?: string) {
  return useQuery({
    queryKey: dataKeys.rows(id ?? "", view, q),
    queryFn: () =>
      unwrap(
        api.GET("/databases/{id}/rows", {
          params: { path: { id: id! }, query: { view: view ?? undefined, q: q || undefined } },
        }),
      ),
    enabled: !!id,
    placeholderData: (prev) => prev,
  });
}

export function useTaskRows(taskId: string | undefined) {
  return useQuery({
    queryKey: dataKeys.task(taskId ?? ""),
    queryFn: () => unwrap(api.GET("/tasks/{id}/rows", { params: { path: { id: taskId! } } })),
    enabled: !!taskId,
  });
}

/** Anything about a database changed: refetch what shows it. */
function useRefresh() {
  const qc = useQueryClient();
  return () => {
    for (const key of [["databases"], ["database"], ["rows"], ["task-rows"]]) qc.invalidateQueries({ queryKey: key });
  };
}

export function useCreateDatabase(projectId: string) {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: (name: string) =>
      unwrap(api.POST("/projects/{id}/databases", { params: { path: { id: projectId } }, body: { name } })),
    onSuccess: refresh,
  });
}

export function useImportDatabase(projectId: string) {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: async ({ file, name }: { file: File; name: string }) => {
      const res = await fetch(`/api/v1/projects/${projectId}/databases/import?name=${encodeURIComponent(name)}`, {
        method: "POST",
        credentials: "same-origin",
        headers: { "Content-Type": "text/csv" },
        body: await file.text(),
      });
      const body = await res.json().catch(() => null);
      if (!res.ok) throw new Error(body?.message ?? "That file couldn't be imported.");
      return body as components["schemas"]["DatabaseDetail"];
    },
    onSuccess: refresh,
  });
}

export function useUpdateDatabase() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: ({ id, ...body }: components["schemas"]["UpdateDatabaseRequest"] & { id: string }) =>
      unwrap(api.PATCH("/databases/{id}", { params: { path: { id } }, body })),
    onSuccess: refresh,
  });
}

export function useDeleteDatabase() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE("/databases/{id}", { params: { path: { id } } })),
    onSuccess: refresh,
  });
}

export function useCreateColumn(databaseId: string) {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: (body: CreateColumn) =>
      unwrap(api.POST("/databases/{id}/columns", { params: { path: { id: databaseId } }, body })),
    onSuccess: refresh,
  });
}

export function useUpdateColumn() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: ({ id, ...body }: UpdateColumn & { id: string }) =>
      unwrap(api.PATCH("/db-columns/{id}", { params: { path: { id } }, body })),
    onSuccess: refresh,
  });
}

export function useDeleteColumn() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE("/db-columns/{id}", { params: { path: { id } } })),
    onSuccess: refresh,
  });
}

export function useCreateView(databaseId: string) {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: (body: CreateView) => unwrap(api.POST("/databases/{id}/views", { params: { path: { id: databaseId } }, body })),
    onSuccess: refresh,
  });
}

export function useUpdateView() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: ({ id, ...body }: UpdateView & { id: string }) =>
      unwrap(api.PATCH("/db-views/{id}", { params: { path: { id } }, body })),
    onSuccess: refresh,
  });
}

export function useDeleteView() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE("/db-views/{id}", { params: { path: { id } } })),
    onSuccess: refresh,
  });
}

export function useCreateRow(databaseId: string) {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: (body: CreateRow) => unwrap(api.POST("/databases/{id}/rows", { params: { path: { id: databaseId } }, body })),
    onSuccess: refresh,
  });
}

/** Change a row. The cached lists show the change at once. */
export function useUpdateRow() {
  const qc = useQueryClient();
  const refresh = useRefresh();
  return useMutation({
    mutationFn: ({ id, ...body }: UpdateRow & { id: string }) =>
      unwrap(api.PATCH("/db-rows/{id}", { params: { path: { id } }, body })),
    onMutate: ({ id, values }) => {
      if (!values) return;
      qc.setQueriesData<Row[]>({ queryKey: ["rows"] }, (rows) =>
        rows?.map((r) => {
          if (r.id !== id) return r;
          const next = { ...(r.values as Record<string, unknown>) };
          for (const [k, v] of Object.entries(values)) {
            if (v === null) delete next[k];
            else next[k] = v;
          }
          return { ...r, values: next };
        }),
      );
    },
    onSettled: refresh,
  });
}

export function useDeleteRow() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE("/db-rows/{id}", { params: { path: { id } } })),
    onSuccess: refresh,
  });
}
