import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, unwrap, type CanvasNode, type CanvasView, type Link, type Project } from "../api/client";
import type { components } from "../api/schema";

type CreateProject = components["schemas"]["CreateProjectRequest"];
type UpdateProject = components["schemas"]["UpdateProjectRequest"];
type UpdateLink = components["schemas"]["UpdateLinkRequest"];
type NodePosition = components["schemas"]["NodePosition"];
type GroupPosition = components["schemas"]["GroupPosition"];
type CreateGroup = components["schemas"]["CreateGroupRequest"];
type UpdateGroup = components["schemas"]["UpdateGroupRequest"];

export const canvasKeys = {
  all: ["canvas"] as const,
  home: (includeArchived: boolean) => ["canvas", "home", { includeArchived }] as const,
  project: (id: string) => ["project", id] as const,
};

export function useCanvas(includeArchived: boolean) {
  return useQuery({
    queryKey: canvasKeys.home(includeArchived),
    queryFn: () => unwrap(api.GET("/canvases/home", { params: { query: { include_archived: includeArchived } } })),
  });
}

export function useProjectDetail(id: string | null) {
  return useQuery({
    queryKey: canvasKeys.project(id ?? ""),
    queryFn: () => unwrap(api.GET("/projects/{id}", { params: { path: { id: id! } } })),
    enabled: !!id,
  });
}

/** Refetch everything a project or link change can affect. */
function useRefresh() {
  const qc = useQueryClient();
  return () => {
    qc.invalidateQueries({ queryKey: canvasKeys.all });
    qc.invalidateQueries({ queryKey: ["project"] });
  };
}

/** Apply a change to every cached canvas view, for instant feedback. */
function useEditCanvases() {
  const qc = useQueryClient();
  return (edit: (view: CanvasView) => CanvasView) => {
    for (const [key, view] of qc.getQueriesData<CanvasView>({ queryKey: canvasKeys.all })) {
      if (view) qc.setQueryData(key, edit(view));
    }
  };
}

export function useCreateProject() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: (body: CreateProject) => unwrap(api.POST("/projects", { body })),
    onSuccess: refresh,
  });
}

export function useUpdateProject() {
  const refresh = useRefresh();
  const edit = useEditCanvases();
  return useMutation({
    mutationFn: ({ id, ...body }: UpdateProject & { id: string }) =>
      unwrap(api.PATCH("/projects/{id}", { params: { path: { id } }, body })),
    onSuccess: (project: Project) => {
      edit((v) => ({ ...v, projects: v.projects.map((p) => (p.id === project.id ? project : p)) }));
      refresh();
    },
  });
}

export function useDeleteProject() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE("/projects/{id}", { params: { path: { id } } })),
    onSuccess: refresh,
  });
}

export function useCreateLink() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: (body: { source_id: string; target_id: string; kind: string; label?: string | null }) =>
      unwrap(api.POST("/links", { body })),
    onSuccess: refresh,
  });
}

export function useUpdateLink() {
  const refresh = useRefresh();
  const edit = useEditCanvases();
  return useMutation({
    mutationFn: ({ id, ...body }: UpdateLink & { id: string }) =>
      unwrap(api.PATCH("/links/{id}", { params: { path: { id } }, body })),
    onSuccess: (link: Link) => {
      edit((v) => ({ ...v, links: v.links.map((l) => (l.id === link.id ? link : l)) }));
      refresh();
    },
  });
}

export function useDeleteLink() {
  const refresh = useRefresh();
  const edit = useEditCanvases();
  return useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE("/links/{id}", { params: { path: { id } } })),
    onMutate: (id) => edit((v) => ({ ...v, links: v.links.filter((l) => l.id !== id) })),
    onSettled: refresh,
  });
}

/** Saves card and group positions. The cache is updated first so nothing jumps back. */
export function useSaveNodes() {
  const edit = useEditCanvases();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ canvasId, nodes, groups = [] }: { canvasId: string; nodes: NodePosition[]; groups?: GroupPosition[] }) =>
      unwrap(api.PATCH("/canvases/{id}/nodes", { params: { path: { id: canvasId } }, body: { nodes, groups } })),
    onMutate: ({ nodes, groups = [] }) => {
      const moved = new Map(nodes.map((n) => [n.project_id, n]));
      const movedGroups = new Map(groups.map((g) => [g.id, g]));
      edit((v) => ({
        ...v,
        nodes: v.nodes.map((n) => {
          const g = n.kind === "group" ? movedGroups.get(n.id) : undefined;
          if (g) return { ...n, x: g.x, y: g.y, width: g.width ?? n.width, height: g.height ?? n.height };
          const m = n.project_id ? moved.get(n.project_id) : undefined;
          return m ? { ...n, x: m.x, y: m.y } : n;
        }),
      }));
    },
    onError: () => qc.invalidateQueries({ queryKey: canvasKeys.all }),
  });
}

export function useCreateGroup() {
  const edit = useEditCanvases();
  return useMutation({
    mutationFn: ({ canvasId, ...body }: CreateGroup & { canvasId: string }) =>
      unwrap(api.POST("/canvases/{id}/groups", { params: { path: { id: canvasId } }, body })),
    onSuccess: (group: CanvasNode) => edit((v) => ({ ...v, nodes: [...v.nodes, group] })),
  });
}

export function useUpdateGroup() {
  const edit = useEditCanvases();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ canvasId, groupId, ...body }: UpdateGroup & { canvasId: string; groupId: string }) =>
      unwrap(api.PATCH("/canvases/{id}/groups/{group_id}", { params: { path: { id: canvasId, group_id: groupId } }, body })),
    onMutate: ({ groupId, label, color }) =>
      edit((v) => ({
        ...v,
        nodes: v.nodes.map((n) => (n.id === groupId ? { ...n, label: label ?? n.label, color: color ?? n.color } : n)),
      })),
    onError: () => qc.invalidateQueries({ queryKey: canvasKeys.all }),
  });
}

export function useDeleteGroup() {
  const edit = useEditCanvases();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ canvasId, groupId }: { canvasId: string; groupId: string }) =>
      unwrap(api.DELETE("/canvases/{id}/groups/{group_id}", { params: { path: { id: canvasId, group_id: groupId } } })),
    onMutate: ({ groupId }) => edit((v) => ({ ...v, nodes: v.nodes.filter((n) => n.id !== groupId) })),
    onError: () => qc.invalidateQueries({ queryKey: canvasKeys.all }),
  });
}

export type LayoutSnapshot = components["schemas"]["LayoutSnapshot"];

/** Lay out what one person shares with you the way they have it. */
export function useCopyLayout() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: (ownerId: string) => unwrap(api.POST("/canvases/home/copy-layout", { body: { owner_id: ownerId } })),
    onSuccess: refresh,
  });
}

/** Undo a layout copy. */
export function useRestoreLayout() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: (snapshot: LayoutSnapshot) => unwrap(api.POST("/canvases/home/restore-layout", { body: snapshot })),
    onSuccess: refresh,
  });
}
