import type { ColorMode, Theme } from "./types";

/** Every CSS variable the app reads is prefixed with this. */
export const VAR_PREFIX = "--tmp-";

/** Replaces each `{primitive}` reference inside a value. */
export function substitute(value: string, primitives: Record<string, string>): string {
  return value.replace(/\{([a-z0-9-]+)\}/gi, (match, name: string) => primitives[name] ?? match);
}

/** Picks the mode to use: the one asked for if the theme has it, else whichever it has. */
export function pickMode(theme: Theme, wanted: ColorMode): ColorMode {
  if (theme.modes[wanted]) return wanted;
  return wanted === "dark" ? "light" : "dark";
}

/**
 * Turns a theme into CSS custom properties: semantic and component tokens,
 * project colour slots, shape and fonts. Primitives are never exposed, so
 * components can only use semantic names.
 */
export function resolveTheme(theme: Theme, wanted: ColorMode): { mode: ColorMode; vars: Record<string, string> } {
  const mode = pickMode(theme, wanted);
  const m = theme.modes[mode];
  if (!m) throw new Error(`Theme ${theme.id} has no modes`);
  const sub = (v: string) => substitute(v, theme.primitives);
  const vars: Record<string, string> = {};
  for (const [k, v] of Object.entries(m.semantic)) vars[`${VAR_PREFIX}${k}`] = sub(v);
  for (const [k, v] of Object.entries(m.component)) vars[`${VAR_PREFIX}${k}`] = sub(v);
  m.slots.forEach((v, i) => (vars[`${VAR_PREFIX}slot-${i + 1}`] = sub(v)));
  for (const [k, v] of Object.entries(theme.shape)) vars[`${VAR_PREFIX}${k}`] = sub(v);
  for (const [k, v] of Object.entries(theme.fonts)) vars[`${VAR_PREFIX}font-${k}`] = v;
  return { mode, vars };
}

/** CSS colour for a project's stored colour: `slot-3` or a one-off `#rrggbb`. */
export function projectColor(color: string): string {
  const slot = /^slot-([1-8])$/.exec(color);
  if (slot) return `var(${VAR_PREFIX}slot-${slot[1]})`;
  if (/^#[0-9a-f]{6}$/i.test(color)) return color;
  return `var(${VAR_PREFIX}slot-1)`;
}
