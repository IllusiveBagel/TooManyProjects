import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, unwrap } from "../api/client";
import type { components } from "../api/schema";

type CreateTemplate = components["schemas"]["CreateTemplateRequest"];
type UpdateTemplate = components["schemas"]["UpdateTemplateRequest"];

const KEY = ["templates"] as const;

export function useTemplates() {
  return useQuery({
    queryKey: KEY,
    queryFn: () => unwrap(api.GET("/templates")),
    staleTime: 30_000,
  });
}

export function useCreateTemplate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateTemplate) =>
      unwrap(api.POST("/templates", { body })),
    onSuccess: () => qc.invalidateQueries({ queryKey: KEY }),
  });
}

export function useUpdateTemplate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, ...body }: UpdateTemplate & { id: string }) =>
      unwrap(api.PATCH("/templates/{id}", { params: { path: { id } }, body })),
    onSuccess: () => qc.invalidateQueries({ queryKey: KEY }),
  });
}

export function useDeleteTemplate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) =>
      unwrap(api.DELETE("/templates/{id}", { params: { path: { id } } })),
    onSuccess: () => qc.invalidateQueries({ queryKey: KEY }),
  });
}

/** Switch a project with an empty board to a template. Everything about the project can change, so refetch it all. */
export function useApplyTemplate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      projectId,
      templateId,
    }: {
      projectId: string;
      templateId: string;
    }) =>
      unwrap(
        api.POST("/projects/{id}/apply-template", {
          params: { path: { id: projectId } },
          body: { template_id: templateId },
        }),
      ),
    onSuccess: () => qc.invalidateQueries(),
  });
}
