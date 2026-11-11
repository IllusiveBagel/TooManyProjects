import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, unwrap } from "../api/client";
import type { components } from "../api/schema";

export type Share = components["schemas"]["Share"];
export type Member = components["schemas"]["Member"];
export type Person = components["schemas"]["Person"];
type CreateShare = components["schemas"]["CreateShareRequest"];
type UpdateShare = components["schemas"]["UpdateShareRequest"];

export const shareKeys = {
  people: ["people"] as const,
  shares: ["shares"] as const,
  members: (projectId: string) => ["members", projectId] as const,
};

/** Everyone else with an account here. */
export function usePeople() {
  return useQuery({ queryKey: shareKeys.people, queryFn: () => unwrap(api.GET("/people")) });
}

/** Shares you made and shares made with you. */
export function useShares() {
  return useQuery({ queryKey: shareKeys.shares, queryFn: () => unwrap(api.GET("/shares")) });
}

/** Everyone who can see a project, owner first. */
export function useMembers(projectId: string | undefined) {
  return useQuery({
    queryKey: shareKeys.members(projectId ?? ""),
    queryFn: () => unwrap(api.GET("/projects/{id}/people", { params: { path: { id: projectId! } } })),
    enabled: !!projectId,
  });
}

/** A share changes what everyone can see, so refetch nearly everything. */
function useRefresh() {
  const qc = useQueryClient();
  return () => {
    for (const key of [["shares"], ["members"], ["canvas"], ["project"], ["tasks"], ["notes"]]) {
      qc.invalidateQueries({ queryKey: key });
    }
  };
}

export function useCreateShare() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: (body: CreateShare) => unwrap(api.POST("/shares", { body })),
    onSuccess: refresh,
  });
}

export function useUpdateShare() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: ({ id, ...body }: UpdateShare & { id: string }) =>
      unwrap(api.PATCH("/shares/{id}", { params: { path: { id } }, body })),
    onSuccess: refresh,
  });
}

export function useDeleteShare() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE("/shares/{id}", { params: { path: { id } } })),
    onSuccess: refresh,
  });
}

export type Exclusion = components["schemas"]["Exclusion"];

/** Take one project away from someone who gets it through a wider share. */
export function useExclude() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: ({ projectId, userId }: { projectId: string; userId: string }) =>
      unwrap(api.PUT("/projects/{id}/exclusions/{user_id}", { params: { path: { id: projectId, user_id: userId } } })),
    onSuccess: refresh,
  });
}

/** Let someone back into a project you removed them from. */
export function useLetBackIn() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: ({ projectId, userId }: { projectId: string; userId: string }) =>
      unwrap(api.DELETE("/projects/{id}/exclusions/{user_id}", { params: { path: { id: projectId, user_id: userId } } })),
    onSuccess: refresh,
  });
}
