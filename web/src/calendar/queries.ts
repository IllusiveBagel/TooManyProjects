import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, unwrap } from "../api/client";
import type { components } from "../api/schema";

export type CreateEvent = components["schemas"]["CreateEventRequest"];
export type UpdateEvent = components["schemas"]["UpdateEventRequest"];

export interface CalendarFilter {
  from: string;
  to: string;
  project_id?: string;
  mine?: boolean;
}

export const calendarKeys = {
  all: ["calendar"] as const,
  range: (f: CalendarFilter) => ["calendar", f] as const,
};

export function useCalendar(filter: CalendarFilter, enabled = true) {
  return useQuery({
    queryKey: calendarKeys.range(filter),
    queryFn: () => unwrap(api.GET("/calendar", { params: { query: filter } })),
    enabled,
  });
}

/** A calendar change shows in every range and in projects' activity. */
function useRefresh() {
  const qc = useQueryClient();
  return () => {
    qc.invalidateQueries({ queryKey: calendarKeys.all });
    qc.invalidateQueries({ queryKey: ["dashboard"] });
  };
}

export function useCreateEvent() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: (body: CreateEvent) => unwrap(api.POST("/calendar/events", { body })),
    onSuccess: refresh,
  });
}

export function useUpdateEvent() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: UpdateEvent }) =>
      unwrap(api.PATCH("/calendar/events/{id}", { params: { path: { id } }, body })),
    onSuccess: refresh,
  });
}

export function useDeleteEvent() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: ({ id, occurrence }: { id: string; occurrence?: string }) =>
      unwrap(api.DELETE("/calendar/events/{id}", { params: { path: { id }, query: occurrence ? { occurrence } : {} } })),
    onSuccess: refresh,
  });
}

export function useJoin() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: ({ id, occurrence, join }: { id: string; occurrence: string; join: boolean }) => {
      const params = { params: { path: { id, occurrence } } };
      return unwrap(
        join
          ? api.PUT("/calendar/events/{id}/joins/{occurrence}", params)
          : api.DELETE("/calendar/events/{id}/joins/{occurrence}", params),
      );
    },
    onSuccess: refresh,
  });
}
