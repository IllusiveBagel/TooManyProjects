import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { api, unwrap, type Status, type Task, type TaskDetail } from "../api/client";
import type { components } from "../api/schema";
import { moveColumn } from "./columns";
import { applyMove } from "./format";

type CreateTask = components["schemas"]["CreateTaskRequest"];
type UpdateTask = components["schemas"]["UpdateTaskRequest"];
type CreateStatus = components["schemas"]["CreateStatusRequest"];
type UpdateStatus = components["schemas"]["UpdateStatusRequest"];

export interface GlobalFilter {
  project_id?: string;
  family?: string;
  /** Only tasks assigned to you. */
  mine?: boolean;
}

export const taskKeys = {
  statuses: (projectId: string) => ["statuses", projectId] as const,
  labels: (projectId: string) => ["labels", projectId] as const,
  projectTasks: (projectId: string) => ["tasks", "project", projectId] as const,
  global: (filter: GlobalFilter) => ["tasks", "global", filter] as const,
  task: (id: string) => ["task", id] as const,
  dashboard: (projectId: string) => ["dashboard", projectId] as const,
};

export function useStatuses(projectId: string | undefined) {
  return useQuery({
    queryKey: taskKeys.statuses(projectId ?? ""),
    queryFn: () => unwrap(api.GET("/projects/{id}/statuses", { params: { path: { id: projectId! } } })),
    enabled: !!projectId,
  });
}

export function useLabels(projectId: string | undefined) {
  return useQuery({
    queryKey: taskKeys.labels(projectId ?? ""),
    queryFn: () => unwrap(api.GET("/projects/{id}/labels", { params: { path: { id: projectId! } } })),
    enabled: !!projectId,
  });
}

export function useProjectTasks(projectId: string) {
  return useQuery({
    queryKey: taskKeys.projectTasks(projectId),
    queryFn: () => unwrap(api.GET("/projects/{id}/tasks", { params: { path: { id: projectId } } })),
  });
}

export function useGlobalTasks(filter: GlobalFilter) {
  return useQuery({
    queryKey: taskKeys.global(filter),
    queryFn: () => unwrap(api.GET("/tasks", { params: { query: filter } })),
  });
}

export function useTaskDetail(id: string | null) {
  return useQuery({
    queryKey: taskKeys.task(id ?? ""),
    queryFn: () => unwrap(api.GET("/tasks/{id}", { params: { path: { id: id! } } })),
    enabled: !!id,
  });
}

export function useDashboard(projectId: string) {
  return useQuery({
    queryKey: taskKeys.dashboard(projectId),
    queryFn: () => unwrap(api.GET("/projects/{id}/dashboard", { params: { path: { id: projectId } } })),
  });
}

/** Everything a task change can touch: lists, the task, dashboards, card stats. */
function refreshTasks(qc: QueryClient) {
  for (const key of [["tasks"], ["task"], ["dashboard"], ["canvas"], ["project"]]) {
    qc.invalidateQueries({ queryKey: key });
  }
}

/** Swaps a task into every cached list and detail, so edits show at once. */
function putTask(qc: QueryClient, task: Task) {
  for (const [key, list] of qc.getQueriesData<Task[]>({ queryKey: ["tasks"] })) {
    if (list) qc.setQueryData(key, list.map((t) => (t.id === task.id ? task : t)));
  }
  qc.setQueryData<TaskDetail>(taskKeys.task(task.id), (d) => (d ? { ...d, task } : d));
}

export function useCreateTask() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ projectId, ...body }: CreateTask & { projectId: string }) =>
      unwrap(api.POST("/projects/{id}/tasks", { params: { path: { id: projectId } }, body })),
    onSuccess: () => refreshTasks(qc),
  });
}

export function useUpdateTask() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, ...body }: UpdateTask & { id: string }) =>
      unwrap(api.PATCH("/tasks/{id}", { params: { path: { id } }, body })),
    onSuccess: (task) => {
      putTask(qc, task);
      refreshTasks(qc);
    },
  });
}

/**
 * Moves a card on a project board. The cached list changes before the call
 * goes out, so the card never snaps back while the server answers.
 */
export function useMoveTask(projectId: string) {
  const qc = useQueryClient();
  const update = useUpdateTask();
  return (taskId: string, status: Status, afterId: string | null) => {
    const key = taskKeys.projectTasks(projectId);
    qc.setQueryData<Task[]>(key, (list) => (list ? applyMove(list, taskId, status, afterId) : list));
    update.mutate({ id: taskId, status_id: status.id, after_id: afterId }, { onError: () => refreshTasks(qc) });
  };
}

/** Moves a card on the global board to a category's column. */
export function useMoveGlobal(filter: GlobalFilter) {
  const qc = useQueryClient();
  const update = useUpdateTask();
  return (taskId: string, category: string) => {
    qc.setQueryData<Task[]>(taskKeys.global(filter), (list) =>
      list?.map((t) => (t.id === taskId ? { ...t, category, overdue: category === "done" ? false : t.overdue } : t)),
    );
    update.mutate({ id: taskId, category }, { onError: () => refreshTasks(qc) });
  };
}

export function useDeleteTask() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE("/tasks/{id}", { params: { path: { id } } })),
    onSuccess: () => refreshTasks(qc),
  });
}

// Columns ---------------------------------------------------------------------

function refreshBoard(qc: QueryClient, projectId: string) {
  qc.invalidateQueries({ queryKey: taskKeys.statuses(projectId) });
  refreshTasks(qc);
}

export function useCreateStatus(projectId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateStatus) =>
      unwrap(api.POST("/projects/{id}/statuses", { params: { path: { id: projectId } }, body })),
    onSuccess: () => refreshBoard(qc, projectId),
  });
}

export function useUpdateStatus(projectId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, ...body }: UpdateStatus & { id: string }) =>
      unwrap(api.PATCH("/statuses/{id}", { params: { path: { id } }, body })),
    onSuccess: () => refreshBoard(qc, projectId),
  });
}

/** Move a column: shown at once, then saved; a failed save puts it back. */
export function useMoveStatus(projectId: string) {
  const qc = useQueryClient();
  const key = taskKeys.statuses(projectId);
  return useMutation({
    mutationFn: ({ id, afterId }: { id: string; afterId: string | null }) =>
      unwrap(api.PATCH("/statuses/{id}", { params: { path: { id } }, body: { after_id: afterId } })),
    onMutate: async ({ id, afterId }) => {
      await qc.cancelQueries({ queryKey: key });
      const before = qc.getQueryData<Status[]>(key);
      qc.setQueryData<Status[]>(key, (list) => (list ? moveColumn(list, id, afterId) : list));
      return { before };
    },
    onError: (_e, _v, ctx) => ctx?.before && qc.setQueryData(key, ctx.before),
    onSettled: () => refreshBoard(qc, projectId),
  });
}

export function useDeleteStatus(projectId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, moveTo }: { id: string; moveTo?: string }) =>
      unwrap(api.DELETE("/statuses/{id}", { params: { path: { id }, query: { move_to: moveTo } } })),
    onSuccess: () => refreshBoard(qc, projectId),
  });
}

// Labels and checklist ---------------------------------------------------------

export function useCreateLabel(projectId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: { name: string; color?: string | null }) =>
      unwrap(api.POST("/projects/{id}/labels", { params: { path: { id: projectId } }, body })),
    onSuccess: () => qc.invalidateQueries({ queryKey: taskKeys.labels(projectId) }),
  });
}

export function useChecklist(taskId: string) {
  const qc = useQueryClient();
  const onSuccess = () => {
    qc.invalidateQueries({ queryKey: taskKeys.task(taskId) });
    qc.invalidateQueries({ queryKey: ["tasks"] });
  };
  const add = useMutation({
    mutationFn: (text: string) =>
      unwrap(api.POST("/tasks/{id}/checklist", { params: { path: { id: taskId } }, body: { text } })),
    onSuccess,
  });
  const update = useMutation({
    mutationFn: ({ id, ...body }: { id: string; text?: string; done?: boolean }) =>
      unwrap(api.PATCH("/checklist/{id}", { params: { path: { id } }, body })),
    onMutate: ({ id, done }) => {
      if (done === undefined) return;
      qc.setQueryData<TaskDetail>(taskKeys.task(taskId), (d) =>
        d ? { ...d, checklist: d.checklist.map((c) => (c.id === id ? { ...c, done } : c)) } : d,
      );
    },
    onSettled: onSuccess,
  });
  const remove = useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE("/checklist/{id}", { params: { path: { id } } })),
    onSuccess,
  });
  return { add, update, remove };
}

// Comments and linked notes ------------------------------------------------------

/** After a comment or link changes: the task, card counts, and notes that list tasks. */
function refreshLog(qc: QueryClient, taskId: string) {
  qc.invalidateQueries({ queryKey: taskKeys.task(taskId) });
  qc.invalidateQueries({ queryKey: ["tasks"] });
  qc.invalidateQueries({ queryKey: ["dashboard"] });
}

export function useComments(taskId: string) {
  const qc = useQueryClient();
  const onSuccess = () => refreshLog(qc, taskId);
  const add = useMutation({
    mutationFn: (body: string) =>
      unwrap(api.POST("/tasks/{id}/comments", { params: { path: { id: taskId } }, body: { body } })),
    onSuccess,
  });
  const update = useMutation({
    mutationFn: ({ id, body }: { id: string; body: string }) =>
      unwrap(api.PATCH("/comments/{id}", { params: { path: { id } }, body: { body } })),
    onSuccess,
  });
  const remove = useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE("/comments/{id}", { params: { path: { id } } })),
    onSuccess,
  });
  return { add, update, remove };
}

export function useTaskNoteLinks() {
  const qc = useQueryClient();
  const onSuccess = (_: unknown, { taskId, noteId }: { taskId: string; noteId: string }) => {
    refreshLog(qc, taskId);
    qc.invalidateQueries({ queryKey: ["notes", "one", noteId] });
  };
  const link = useMutation({
    mutationFn: ({ taskId, noteId }: { taskId: string; noteId: string }) =>
      unwrap(api.PUT("/tasks/{id}/notes/{note_id}", { params: { path: { id: taskId, note_id: noteId } } })),
    onSuccess,
  });
  const unlink = useMutation({
    mutationFn: ({ taskId, noteId }: { taskId: string; noteId: string }) =>
      unwrap(api.DELETE("/tasks/{id}/notes/{note_id}", { params: { path: { id: taskId, note_id: noteId } } })),
    onSuccess,
  });
  return { link, unlink };
}
