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

export type HttpsInfo = components["schemas"]["HttpsInfo"];

const httpsKey = ["system", "https"] as const;

/** Whether the server serves HTTPS itself, and what its certificate covers. */
export function useHttpsInfo() {
  return useQuery({ queryKey: httpsKey, queryFn: () => unwrap(api.GET("/https")), staleTime: 60_000 });
}

export function useAddHttpsHost() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (host: string) => unwrap(api.POST("/https/hosts", { body: { host } })),
    onSuccess: (info) => qc.setQueryData(httpsKey, info),
  });
}

export function useRemoveHttpsHost() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (host: string) => unwrap(api.DELETE("/https/hosts/{host}", { params: { path: { host } } })),
    onSuccess: (info) => qc.setQueryData(httpsKey, info),
  });
}

/**
 * The address to give Claude Code: this page's own when it is already
 * https, otherwise the same host on the server's HTTPS port.
 */
export function httpsOrigin(location: Pick<Location, "protocol" | "hostname" | "origin">, info: HttpsInfo | undefined): string | null {
  if (location.protocol === "https:") return location.origin;
  if (!info || info.mode === "off" || !info.port) return null;
  const host = location.hostname.includes(":") ? `[${location.hostname}]` : location.hostname;
  return `https://${host}${info.port === 443 ? "" : `:${info.port}`}`;
}

/** Whether the app's own certificate covers `hostname`. Unknown (own certificate, or off) counts as covered. */
export function covers(info: HttpsInfo | undefined, hostname: string): boolean {
  if (!info || info.mode !== "generated") return true;
  const h = hostname.replace(/^\[|\]$/g, "").toLowerCase();
  return info.hosts.some((x) => x.host === h);
}
