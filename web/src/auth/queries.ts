import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createContext, useContext } from "react";
import { api, unwrap, type User } from "../api/client";

export const authKeys = { status: ["auth", "status"] as const };

export function useAuthStatus() {
  return useQuery({
    queryKey: authKeys.status,
    queryFn: () => unwrap(api.GET("/auth/status")),
    staleTime: 60_000,
  });
}

/** Set by the signed-in route gate, so pages never see a missing user mid sign-out. */
export const MeContext = createContext<User | null>(null);

/** The signed-in person. Only call inside the signed-in part of the app. */
export function useMe(): User {
  const user = useContext(MeContext);
  if (!user) throw new Error("useMe outside a signed-in route");
  return user;
}

function useSetUser() {
  const qc = useQueryClient();
  return (user: User | null) => qc.setQueryData(authKeys.status, { needs_setup: false, user });
}

export function useSetup() {
  const setUser = useSetUser();
  return useMutation({
    mutationFn: (body: { name: string; email: string; password: string }) =>
      unwrap(api.POST("/auth/setup", { body })),
    onSuccess: setUser,
  });
}

export function useLogin() {
  const setUser = useSetUser();
  return useMutation({
    mutationFn: (body: { email: string; password: string }) => unwrap(api.POST("/auth/login", { body })),
    onSuccess: setUser,
  });
}

export function useAcceptInvite() {
  const setUser = useSetUser();
  return useMutation({
    mutationFn: (body: { token: string; name: string; email: string; password: string }) =>
      unwrap(api.POST("/auth/accept-invite", { body })),
    onSuccess: setUser,
  });
}

export function useLogout() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(api.POST("/auth/logout")),
    onSuccess: () => {
      qc.setQueryData(authKeys.status, { needs_setup: false, user: null });
      qc.removeQueries({ predicate: (q) => q.queryKey[0] !== "auth" });
    },
  });
}

export function useUpdatePreferences() {
  const setUser = useSetUser();
  return useMutation({
    mutationFn: (body: { theme_id?: string; theme_mode?: string }) =>
      unwrap(api.PATCH("/me/preferences", { body })),
    onSuccess: setUser,
  });
}

const sessionsKey = ["sessions"] as const;

/** The browsers and devices signed in to this account, this one first. */
export function useSessions() {
  return useQuery({ queryKey: sessionsKey, queryFn: () => unwrap(api.GET("/me/sessions")) });
}

export function useChangePassword() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: { current_password: string; new_password: string }) =>
      unwrap(api.POST("/me/password", { body })),
    onSuccess: () => qc.invalidateQueries({ queryKey: sessionsKey }),
  });
}

export function useEndSession() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE("/me/sessions/{id}", { params: { path: { id } } })),
    onSuccess: () => qc.invalidateQueries({ queryKey: sessionsKey }),
  });
}

export function useEndOtherSessions() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(api.POST("/me/sessions/sign-out-others")),
    onSuccess: () => qc.invalidateQueries({ queryKey: sessionsKey }),
  });
}
