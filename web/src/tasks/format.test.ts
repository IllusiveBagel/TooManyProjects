import { describe, expect, it } from "vitest";
import type { ActivityItem, Task } from "../api/client";
import { activityText, applyMove, cardAbove, findColumn, groupByColumn, moveCard } from "./format";

const task = (id: string, status_id: string, category = "todo") => ({ id, status_id, category, overdue: true }) as Task;

describe("board ordering", () => {
  const order = { a: ["1", "2", "3"], b: ["4"], c: [] as string[] };

  it("groups tasks by column and ignores unknown columns", () => {
    const tasks = [task("1", "a"), task("2", "b"), task("3", "a"), task("9", "zzz")];
    expect(groupByColumn(tasks, ["a", "b", "c"], (t) => t.status_id)).toEqual({ a: ["1", "3"], b: ["2"], c: [] });
  });

  it("finds the column of a card or a column id", () => {
    expect(findColumn(order, "4")).toBe("b");
    expect(findColumn(order, "c")).toBe("c");
    expect(findColumn(order, "nope")).toBeUndefined();
  });

  it("moves cards within and between columns", () => {
    expect(moveCard(order, "1", "a", 2)).toEqual({ a: ["2", "3", "1"], b: ["4"], c: [] });
    expect(moveCard(order, "2", "b", 0)).toEqual({ a: ["1", "3"], b: ["2", "4"], c: [] });
    expect(moveCard(order, "2", "c", 99)).toEqual({ a: ["1", "3"], b: ["4"], c: ["2"] });
  });

  it("names the card above a drop", () => {
    expect(cardAbove(order, "a", "1")).toBeNull();
    expect(cardAbove(order, "a", "3")).toBe("2");
  });

  it("applies a move to the cached list", () => {
    const tasks = [task("1", "a"), task("2", "a"), task("3", "b")];
    const done = { id: "b", category: "done" };
    const moved = applyMove(tasks, "1", done, "3");
    expect(moved.map((t) => t.id)).toEqual(["2", "3", "1"]);
    expect(moved[2]).toMatchObject({ status_id: "b", category: "done", overdue: false });
    expect(applyMove(tasks, "3", { id: "a", category: "todo" }, null).map((t) => t.id)).toEqual(["3", "1", "2"]);
  });
});

describe("activityText", () => {
  const text = (kind: string, payload: object) => activityText({ kind, payload } as ActivityItem);
  it("reads like a sentence", () => {
    expect(text("task.moved", { title: "Weld", from: "To do", to: "Doing" })).toBe(
      "moved “Weld” from To do to Doing",
    );
    expect(text("task.updated", { title: "Weld", fields: ["title", "due date"] })).toBe(
      "changed the title, due date of “Weld”",
    );
    expect(text("something.new", {})).toBe("something new");
  });
});
