import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, unwrap } from "../api/client";
import type { components } from "../api/schema";

export type VersionInfo = components["schemas"]["VersionInfo"];

const versionKey = ["system", "version"] as const;

/** The running version and the latest release. Only the owner may ask, so pass `enabled` from their role. */
export function useVersionInfo(enabled: boolean) {
  return useQuery({
    queryKey: versionKey,
    queryFn: () => unwrap(api.GET("/system/version")),
    enabled,
    // The server checks GitHub every few hours; asking it hourly is plenty.
    staleTime: 60 * 60_000,
    refetchInterval: 60 * 60_000,
    retry: false,
  });
}

export function useCheckNow() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(api.POST("/system/version/check")),
    onSuccess: (info) => qc.setQueryData(versionKey, info),
  });
}

/** Docs on updating, linked from the banner and Settings. */
export const UPDATE_GUIDE_URL = "https://github.com/IllusiveBagel/TooManyProjects/blob/main/docs/guide.md#updating";

const DISMISS_KEY = "tmp-update-dismissed";

/** The release version the owner last waved away. Storage can be blocked, so failures mean "not dismissed". */
export function dismissedVersion(): string | null {
  try {
    return localStorage.getItem(DISMISS_KEY);
  } catch {
    return null;
  }
}

export function dismissVersion(version: string) {
  try {
    localStorage.setItem(DISMISS_KEY, version);
  } catch {
    // Without storage the banner just comes back on the next visit.
  }
}
