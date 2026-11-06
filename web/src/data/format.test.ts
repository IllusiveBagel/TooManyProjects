import { describe, expect, it } from "vitest";
import { display, monthGrid, parseEmbed, resolveEmbed, total, visibleColumns, type Column, type DatabaseRef, type Row } from "./format";

const col = (id: string, kind: string, extra: Partial<Column> = {}): Column => ({
  id,
  database_id: "d",
  name: id,
  kind,
  options: { choices: [{ id: "a", name: "To order" }, { id: "b", name: "Ordered" }] },
  is_title: false,
  position: id,
  ...extra,
});

const row = (values: Record<string, unknown>, relations: Row["relations"] = {}): Row => ({
  id: "r",
  database_id: "d",
  values,
  relations,
  position: "a",
  created_at: "",
  updated_at: "",
});

describe("database cells", () => {
  it("shows each kind as text", () => {
    expect(display(col("s", "select"), row({ s: "b" }))).toBe("Ordered");
    expect(display(col("m", "multi_select"), row({ m: ["a", "b"] }))).toBe("To order, Ordered");
    expect(display(col("c", "currency", { options: { choices: [], currency: "GBP" } }), row({ c: 42 }))).toBe("£42.00");
    expect(display(col("x", "checkbox"), row({ x: true }))).toBe("Yes");
    expect(display(col("p", "person"), row({ p: "u1" }), { u1: "Sam" })).toBe("Sam");
    expect(display(col("l", "relation"), row({}, { l: [{ kind: "task", id: "t", title: "T-1 Brakes" }] }))).toBe("T-1 Brakes");
    expect(display(col("t", "text"), row({}))).toBe("");
  });

  it("totals numbers and keeps the name column first", () => {
    expect(total(col("n", "number"), [row({ n: 1.5 }), row({ n: 2 }), row({})])).toBe(3.5);
    const cols = [col("b", "text"), col("a", "text", { is_title: true }), col("c", "text")];
    const view = { id: "v", database_id: "d", name: "V", kind: "table", pinned: false, position: "a", config: { filters: [], sorts: [], hidden: ["c", "a"] } };
    expect(visibleColumns(cols, view).map((c) => c.id)).toEqual(["a", "b"]);
  });
});

describe("embeds", () => {
  const dbs: DatabaseRef[] = [
    { id: "1", name: "Parts list", project_id: "p1", project_title: "Car", project_color: "slot-1", views: [] },
    {
      id: "2",
      name: "Parts list",
      project_id: "p2",
      project_title: "Van",
      project_color: "slot-2",
      views: [
        { id: "v1", database_id: "2", name: "All", kind: "table", pinned: false, position: "a", config: { filters: [], sorts: [], hidden: [] } },
        { id: "v2", database_id: "2", name: "To order", kind: "table", pinned: false, position: "b", config: { filters: [], sorts: [], hidden: [] } },
      ],
    },
  ];

  it("reads the database and view", () => {
    expect(parseEmbed("Parts list#To order")).toEqual({ database: "Parts list", view: "To order" });
    expect(parseEmbed("Parts list")).toEqual({ database: "Parts list", view: null });
  });

  it("prefers the note's projects and falls back to the first view", () => {
    expect(resolveEmbed("parts list#to order", dbs, ["p2"])?.view?.id).toBe("v2");
    expect(resolveEmbed("Parts list#Missing", dbs, ["p2"])?.view?.id).toBe("v1");
    expect(resolveEmbed("Parts list", dbs)?.database.id).toBe("1");
    expect(resolveEmbed("Nope", dbs)).toBeNull();
  });
});

describe("calendar", () => {
  it("starts on Monday and fills whole weeks", () => {
    const days = monthGrid(2026, 10); // November 2026 starts on a Sunday.
    expect(days[0]).toEqual({ iso: "2026-10-26", inMonth: false });
    expect(days[6]).toEqual({ iso: "2026-11-01", inMonth: true });
    expect(days.length % 7).toBe(0);
  });
});
