/** A theme token file, as in `themes/workshop.json`. See `themes/theme.schema.json`. */
export interface Theme {
  id: string;
  name: string;
  version: number;
  description?: string;
  fonts: Record<"display" | "interface" | "mono" | "reading", string>;
  /** Raw values. Only the theme refers to them, as `{name}`. */
  primitives: Record<string, string>;
  modes: Partial<Record<ColorMode, ThemeMode>>;
  shape: Record<string, string>;
}

export interface ThemeMode {
  label: string;
  semantic: Record<SemanticToken, string>;
  /** Project colours 1 to 8. A project stores `slot-N`, never a hex code. */
  slots: string[];
  component: Record<string, string>;
}

export type ColorMode = "dark" | "light";
export type ModePreference = ColorMode | "system";

export const SEMANTIC_TOKENS = [
  "surface",
  "surface-card",
  "surface-raised",
  "line",
  "text",
  "text-quiet",
  "accent",
  "on-accent",
  "late",
  "done",
  "focus",
  "shadow",
] as const;
export type SemanticToken = (typeof SEMANTIC_TOKENS)[number];
