import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, unwrap } from "../api/client";
import { browserZone } from "../calendar/format";

export function useShoppingList() {
  return useQuery({
    queryKey: ["shopping"],
    queryFn: () => unwrap(api.GET("/shopping", { params: { query: { tz: browserZone() } } })),
  });
}

export function useProjectSpend(projectId: string | undefined) {
  return useQuery({
    queryKey: ["spend", projectId ?? ""],
    queryFn: () => unwrap(api.GET("/projects/{id}/spend", { params: { path: { id: projectId! } } })),
    enabled: !!projectId,
  });
}

export function useInstanceSettings() {
  return useQuery({ queryKey: ["instance-settings"], queryFn: () => unwrap(api.GET("/instance-settings")), staleTime: 60_000 });
}

export function useUpdateInstanceSettings() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (currency: string) => unwrap(api.PATCH("/instance-settings", { body: { currency } })),
    onSuccess: () => {
      for (const key of [["instance-settings"], ["shopping"], ["spend"], ["today"], ["databases"], ["database"]]) {
        qc.invalidateQueries({ queryKey: key });
      }
    },
  });
}

/** Move parts to a stage together. Refreshes everything that shows parts. */
export function useMarkParts() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: { row_ids: string[]; stage: string; date?: string }) =>
      unwrap(api.POST("/shopping/mark", { body: { ...body, tz: browserZone() } })),
    onSuccess: () => {
      for (const key of [["shopping"], ["spend"], ["today"], ["rows"], ["task-rows"], ["tasks"], ["task"], ["databases"], ["dashboard"]]) {
        qc.invalidateQueries({ queryKey: key });
      }
    },
  });
}
