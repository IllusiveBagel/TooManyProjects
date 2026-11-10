import type { components } from "../api/schema";

export type Part = components["schemas"]["Part"];
export type SupplierGroup = components["schemas"]["SupplierGroup"];
export type ShoppingList = components["schemas"]["ShoppingList"];
export type ProjectSpend = components["schemas"]["ProjectSpend"];

/** The Status stages of a Parts database, in order. */
export const STAGES = ["need", "ordered", "shipped", "arrived", "fitted"] as const;
export type Stage = (typeof STAGES)[number];

export const STAGE_LABEL: Record<string, string> = {
  need: "Need",
  ordered: "Ordered",
  shipped: "Shipped",
  arrived: "Arrived",
  fitted: "Fitted",
};

/** Theme colours per stage, matching the template's choice slots. */
export const STAGE_COLOR: Record<string, string> = {
  need: "var(--tmp-late)",
  ordered: "var(--tmp-slot-6)",
  shipped: "var(--tmp-slot-3)",
  arrived: "var(--tmp-slot-5)",
  fitted: "var(--tmp-done)",
};

/** £158.28 in the install's currency. */
export function money(n: number, currency: string): string {
  try {
    return new Intl.NumberFormat("en-GB", { style: "currency", currency, minimumFractionDigits: 2 }).format(n);
  } catch {
    return `${n.toFixed(2)} ${currency}`;
  }
}

/** ×2, or ×1.5 for a fractional quantity. */
export function qty(n: number): string {
  return `×${Number.isInteger(n) ? n : Number(n.toFixed(2))}`;
}

export const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** What a ticked selection adds up to: "£15.00 from LCSC, for 2 projects". */
export function selectionSummary(parts: Part[]): { total: number; suppliers: string[]; projects: number } {
  const total = Math.round(parts.reduce((n, p) => n + p.total, 0) * 100) / 100;
  const suppliers: string[] = [];
  for (const p of parts) {
    const s = p.supplier ?? "";
    if (s && !suppliers.some((x) => x.toLowerCase() === s.toLowerCase())) suppliers.push(s);
  }
  return { total, suppliers, projects: new Set(parts.map((p) => p.project_id)).size };
}

/** "from LCSC, for 2 projects" under a selection. */
export function selectionText(parts: Part[]): string {
  const { suppliers, projects } = selectionSummary(parts);
  const from =
    suppliers.length === 0 ? "" : suppliers.length === 1 ? `from ${suppliers[0]}` : `from ${suppliers.length} suppliers`;
  const forText = `for ${plural(projects, "project")}`;
  return [from, forText].filter(Boolean).join(", ");
}

/** A task's parts line, such as "2 parts · 1 ordered, 1 needed". */
export function waitingText(w: { parts: number; need: number; ordered: number; shipped: number }): string {
  const bits = [
    w.need ? `${w.need} needed` : "",
    w.ordered ? `${w.ordered} ordered` : "",
    w.shipped ? `${w.shipped} shipped` : "",
  ].filter(Boolean);
  return `${plural(w.parts, "part")} · ${bits.join(", ")}`;
}

/** The Board card's parts badge. */
export function partsBadge(t: { parts_total: number; parts_waiting: number }): string | null {
  if (t.parts_total === 0) return null;
  return t.parts_waiting === 0 ? "Parts in" : `Waiting on ${plural(t.parts_waiting, "part")}`;
}

/** Where a part's row opens in its project's Data tab. */
export const partPath = (p: Part) => `/projects/${p.project_id}/data/${p.database_id}?row=${p.row_id}`;
