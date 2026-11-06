import { useQuery } from "@tanstack/react-query";
import { useEffect } from "react";
import { api, unwrap } from "../api/client";
import type { ModePreference, Theme } from "./types";
import { DEFAULT_THEME, savedThemeId, useThemeStore } from "./store";

export function useThemeFile(id: string) {
  return useQuery({
    queryKey: ["themes", id],
    queryFn: async () => (await unwrap(api.GET("/themes/{id}", { params: { path: { id } } }))) as unknown as Theme,
    enabled: id !== DEFAULT_THEME.id,
    staleTime: Infinity,
  });
}

/**
 * Keeps the applied theme in step with the account's saved choice, or with
 * this browser's last choice before sign-in.
 */
export function ThemeSync({ themeId, mode }: { themeId?: string; mode?: string }) {
  const id = themeId ?? savedThemeId;
  const { data } = useThemeFile(id);
  const setTheme = useThemeStore((s) => s.setTheme);
  const setPreference = useThemeStore((s) => s.setPreference);

  useEffect(() => {
    if (id === DEFAULT_THEME.id) setTheme(DEFAULT_THEME);
    else if (data) setTheme(data);
  }, [id, data, setTheme]);

  useEffect(() => {
    if (mode === "system" || mode === "dark" || mode === "light") setPreference(mode as ModePreference);
  }, [mode, setPreference]);

  return null;
}
