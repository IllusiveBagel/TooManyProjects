import { describe, expect, it } from "vitest";
import workshop from "../../../themes/workshop.json";
import { projectColor, resolveTheme, substitute } from "./resolve";
import { SEMANTIC_TOKENS, type Theme } from "./types";

const theme = workshop as unknown as Theme;

describe("resolveTheme", () => {
  it("resolves Workshop dark to the brand values", () => {
    const { mode, vars } = resolveTheme(theme, "dark");
    expect(mode).toBe("dark");
    expect(vars["--tmp-surface"]).toBe("#121110");
    expect(vars["--tmp-accent"]).toBe("#B8C4A0");
    expect(vars["--tmp-slot-2"]).toBe("#5FA8B0");
    expect(vars["--tmp-card-shadow"]).toBe("5px 5px 0 #000000");
    expect(vars["--tmp-radius-lg"]).toBe("6px");
  });

  it("defines every semantic token and 8 slots in both modes, with no unresolved references", () => {
    for (const m of ["dark", "light"] as const) {
      const { vars } = resolveTheme(theme, m);
      for (const t of SEMANTIC_TOKENS) expect(vars[`--tmp-${t}`]).toBeDefined();
      for (let i = 1; i <= 8; i++) expect(vars[`--tmp-slot-${i}`]).toMatch(/^#/);
      for (const v of Object.values(vars)) expect(v).not.toMatch(/\{[a-z0-9-]+\}/);
    }
  });

  it("falls back to the mode a theme has", () => {
    const darkOnly = { ...theme, modes: { dark: theme.modes.dark } };
    expect(resolveTheme(darkOnly, "light").mode).toBe("dark");
  });
});

describe("helpers", () => {
  it("substitutes primitives inside longer values", () => {
    expect(substitute("1px solid {a}", { a: "#fff" })).toBe("1px solid #fff");
    expect(substitute("{missing}", {})).toBe("{missing}");
  });

  it("maps project colours to slots", () => {
    expect(projectColor("slot-4")).toBe("var(--tmp-slot-4)");
    expect(projectColor("#abcdef")).toBe("#abcdef");
    expect(projectColor("javascript:alert(1)")).toBe("var(--tmp-slot-1)");
  });
});

describe("contrast", () => {
  const lum = (hex: string) => {
    const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
    const [r, g, b] = c.map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4)) as [number, number, number];
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
  };
  const ratio = (a: string, b: string) => {
    const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x) as [number, number];
    return (hi + 0.05) / (lo + 0.05);
  };

  it("keeps body and quiet text readable (4.5:1) on every surface in both modes", () => {
    for (const m of ["dark", "light"] as const) {
      const { vars } = resolveTheme(theme, m);
      for (const s of ["surface", "surface-card", "surface-raised"]) {
        for (const t of ["text", "text-quiet"]) {
          expect(ratio(vars[`--tmp-${t}`]!, vars[`--tmp-${s}`]!), `${m} ${t} on ${s}`).toBeGreaterThanOrEqual(4.5);
        }
      }
      expect(ratio(vars["--tmp-on-accent"]!, vars["--tmp-accent"]!), `${m} on-accent`).toBeGreaterThanOrEqual(4.5);
    }
  });
});
