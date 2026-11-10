import { keepPreviousData, useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { api, unwrap } from "../api/client";
import { browserZone } from "../calendar/format";
import type { components } from "../api/schema";

type QuickAdd = components["schemas"]["QuickAddRequest"];
type FileInbox = components["schemas"]["FileInboxRequest"];

export function useToday() {
  return useQuery({
    queryKey: ["today"],
    queryFn: () => unwrap(api.GET("/today", { params: { query: { tz: browserZone() } } })),
  });
}

export function useSearch(q: string) {
  const query = q.trim();
  return useQuery({
    queryKey: ["search", query],
    queryFn: ({ signal }) => unwrap(api.GET("/search", { params: { query: { q: query, limit: 6 } }, signal })),
    enabled: query.length > 0,
    placeholderData: keepPreviousData,
    staleTime: 10_000,
  });
}

export function useLinkPreview(url: string | null) {
  return useQuery({
    queryKey: ["link-preview", url],
    queryFn: ({ signal }) => unwrap(api.GET("/link-preview", { params: { query: { url: url! } }, signal })),
    enabled: !!url,
    staleTime: Infinity,
    retry: false,
  });
}

/** Everything a capture can show up in. */
function refreshCaptures(qc: QueryClient) {
  for (const key of [["today"], ["tasks"], ["notes"], ["note"], ["project-notes"], ["canvas"], ["dashboard"], ["attachments"], ["search"]]) {
    qc.invalidateQueries({ queryKey: key });
  }
}

export function useQuickAdd() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: QuickAdd) => unwrap(api.POST("/quick-add", { body })),
    onSettled: () => refreshCaptures(qc),
  });
}

export function useFileInbox() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, ...body }: FileInbox & { id: string }) =>
      unwrap(api.POST("/inbox/{id}/file", { params: { path: { id } }, body })),
    onSettled: () => refreshCaptures(qc),
  });
}

export { refreshCaptures };
