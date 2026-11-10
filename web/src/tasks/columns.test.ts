import { describe, expect, it } from "vitest";
import type { Status } from "../api/client";
import { afterForStep, doneWarning, moveColumn, soundsDone } from "./columns";

const col = (id: string, name: string, category: string): Status =>
  ({ id, name, category, project_id: "p", color: null, position: id }) as Status;

const z80 = [
  col("a", "To do", "todo"),
  col("b", "Doing", "in_progress"),
  col("c", "Ready to Order", "in_progress"),
  col("d", "Shipped", "in_progress"),
  col("e", "Testing", "in_progress"),
  col("f", "Done", "in_progress"),
];

describe("done warning", () => {
  it("spots a board where nothing counts as done, and offers the obvious fix", () => {
    const w = doneWarning(z80)!;
    expect(w.kind).toBe("none");
    expect(w.fix?.id).toBe("f");
    expect(w.detail).toContain("“Done” column counts as in progress");
  });

  it("asks you to pick a column when none is named like Done", () => {
    const w = doneWarning([col("a", "To do", "todo"), col("b", "Blocked", "blocked")])!;
    expect(w.kind).toBe("none");
    expect(w.fix).toBeUndefined();
  });

  it("flags a Done-named column in another category even when another counts as done", () => {
    const w = doneWarning([col("a", "Published", "in_progress"), col("b", "Archive", "done")])!;
    expect(w.kind).toBe("misnamed");
    expect(w.fix?.id).toBe("a");
  });

  it("is quiet for a healthy board", () => {
    expect(doneWarning([col("a", "To do", "todo"), col("b", "Done", "done")])).toBeNull();
    expect(soundsDone(" finished ")).toBe(true);
    expect(soundsDone("Done-ish")).toBe(false);
  });
});

describe("moving columns", () => {
  it("works out the neighbour for one step left or right", () => {
    expect(afterForStep(z80, "c", -1)).toBe("a");
    expect(afterForStep(z80, "b", -1)).toBeNull();
    expect(afterForStep(z80, "a", -1)).toBeUndefined();
    expect(afterForStep(z80, "e", 1)).toBe("f");
    expect(afterForStep(z80, "f", 1)).toBeUndefined();
  });

  it("reorders a list the way the server will", () => {
    const names = (l: Status[]) => l.map((s) => s.name);
    expect(names(moveColumn(z80, "e", "b"))).toEqual(["To do", "Doing", "Testing", "Ready to Order", "Shipped", "Done"]);
    expect(names(moveColumn(z80, "f", null))[0]).toBe("Done");
    const back = moveColumn(moveColumn(z80, "e", "b"), "e", "d");
    expect(names(back)).toEqual(names(z80));
  });
});
