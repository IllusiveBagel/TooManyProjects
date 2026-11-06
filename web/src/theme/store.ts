import { create } from "zustand";
import workshop from "../../../themes/workshop.json";
import { resolveTheme, VAR_PREFIX } from "./resolve";
import type { ColorMode, ModePreference, Theme } from "./types";

export const DEFAULT_THEME = workshop as unknown as Theme;
const STORAGE_KEY = "tmp.theme";

interface Saved {
  themeId: string;
  preference: ModePreference;
}

function load(): Saved {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) return { themeId: "workshop", preference: "system", ...(JSON.parse(raw) as Partial<Saved>) };
  } catch {
    // Private windows can refuse storage; the defaults are fine.
  }
  return { themeId: "workshop", preference: "system" };
}

function systemMode(): ColorMode {
  return window.matchMedia?.("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

interface ThemeState {
  theme: Theme;
  preference: ModePreference;
  /** The mode actually showing, after resolving `system` and what the theme offers. */
  mode: ColorMode;
  setTheme: (theme: Theme) => void;
  setPreference: (preference: ModePreference) => void;
  /** Re-applies when the OS switches between light and dark. */
  refresh: () => void;
}

function apply(theme: Theme, preference: ModePreference): ColorMode {
  const wanted = preference === "system" ? systemMode() : preference;
  const { mode, vars } = resolveTheme(theme, wanted);
  const root = document.documentElement;
  // Drop variables from a previous theme that this one does not define.
  for (const name of Array.from(root.style)) {
    if (name.startsWith(VAR_PREFIX) && !(name in vars)) root.style.removeProperty(name);
  }
  for (const [k, v] of Object.entries(vars)) root.style.setProperty(k, v);
  root.dataset.theme = theme.id;
  root.dataset.mode = mode;
  root.style.colorScheme = mode;
  return mode;
}

function save(themeId: string, preference: ModePreference) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ themeId, preference }));
  } catch {
    // Ignore: the account keeps the real preference once signed in.
  }
}

const saved = load();

export const useThemeStore = create<ThemeState>((set, get) => ({
  theme: DEFAULT_THEME,
  preference: saved.preference,
  mode: apply(DEFAULT_THEME, saved.preference),
  setTheme: (theme) => {
    const mode = apply(theme, get().preference);
    save(theme.id, get().preference);
    set({ theme, mode });
  },
  setPreference: (preference) => {
    const mode = apply(get().theme, preference);
    save(get().theme.id, preference);
    set({ preference, mode });
  },
  refresh: () => set({ mode: apply(get().theme, get().preference) }),
}));

/** The theme id remembered in this browser, used before sign-in. */
export const savedThemeId = saved.themeId;

window.matchMedia?.("(prefers-color-scheme: light)").addEventListener("change", () => {
  useThemeStore.getState().refresh();
});
