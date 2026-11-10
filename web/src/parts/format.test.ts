import { describe, expect, it } from "vitest";
import { money, partsBadge, qty, selectionText, waitingText, type Part } from "./format";

const part = (over: Partial<Part>): Part => ({
  row_id: "r",
  database_id: "d",
  database_name: "Parts",
  project_id: "p1",
  project_title: "Z80",
  project_color: "slot-1",
  item: "x",
  qty: 1,
  unit_cost: 1,
  total: 1,
  supplier: "LCSC",
  link: null,
  stage: "need",
  ordered_on: null,
  arrived_on: null,
  tasks: [],
  can_edit: true,
  ...over,
});

describe("parts format", () => {
  it("formats money in the install's currency", () => {
    expect(money(158.28, "GBP")).toBe("£158.28");
    expect(money(5, "USD")).toBe("US$5.00");
  });
  it("shows quantities", () => {
    expect(qty(2)).toBe("×2");
    expect(qty(1.5)).toBe("×1.5");
  });
  it("sums up a selection", () => {
    expect(selectionText([part({}), part({ supplier: "lcsc", project_id: "p2" })])).toBe("from LCSC, for 2 projects");
    expect(selectionText([part({}), part({ supplier: "Amazon" })])).toBe("from 2 suppliers, for 1 project");
    expect(selectionText([part({ supplier: null })])).toBe("for 1 project");
  });
  it("says what a task waits on", () => {
    expect(waitingText({ parts: 2, need: 1, ordered: 1, shipped: 0 })).toBe("2 parts · 1 needed, 1 ordered");
    expect(partsBadge({ parts_total: 0, parts_waiting: 0 })).toBeNull();
    expect(partsBadge({ parts_total: 2, parts_waiting: 1 })).toBe("Waiting on 1 part");
    expect(partsBadge({ parts_total: 2, parts_waiting: 0 })).toBe("Parts in");
  });
});
