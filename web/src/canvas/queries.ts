import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, unwrap, type CanvasView, type Link, type Project } from "../api/client";
import type { components } from "../api/schema";

type CreateProject = components["schemas"]["CreateProjectRequest"];
type UpdateProject = components["schemas"]["UpdateProjectRequest"];
type UpdateLink = components["schemas"]["UpdateLinkRequest"];
type NodePosition = components["schemas"]["NodePosition"];

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

/** Saves card positions. The cache is updated first so cards never jump back. */
export function useSaveNodes() {
  const edit = useEditCanvases();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ canvasId, nodes }: { canvasId: string; nodes: NodePosition[] }) =>
      unwrap(api.PATCH("/canvases/{id}/nodes", { params: { path: { id: canvasId } }, body: { nodes } })),
    onMutate: ({ nodes }) => {
      const moved = new Map(nodes.map((n) => [n.project_id, n]));
      edit((v) => ({
        ...v,
        nodes: v.nodes.map((n) => {
          const m = n.project_id ? moved.get(n.project_id) : undefined;
          return m ? { ...n, x: m.x, y: m.y } : n;
        }),
      }));
    },
    onError: () => qc.invalidateQueries({ queryKey: canvasKeys.all }),
  });
}
