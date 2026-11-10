import { describe, expect, it } from "vitest";
import type { ProjectTemplate } from "../api/client";
import { bringsText, compactLine, listJoin } from "./format";

const col = (name: string, category = "in_progress") => ({ name, category });
const base: ProjectTemplate = {
  id: "builtin-pcb",
  name: "Electronics / PCB",
  blurb: "",
  builtin: true,
  mine: false,
  shared: false,
  columns: [
    "Design",
    "Ready to order",
    "Ordered",
    "Assembly",
    "Testing",
    "Done",
  ].map((n) => col(n)),
  label_names: ["Schematic", "Layout", "Firmware"],
  task_count: 0,
  database_names: ["Parts"],
  note_titles: [],
};

describe("templates", () => {
  it("joins lists the way people write them", () => {
    expect(listJoin([])).toBe("");
    expect(listJoin(["a"])).toBe("a");
    expect(listJoin(["a", "b", "c"])).toBe("a, b and c");
  });

  it("says what a template brings", () => {
    expect(bringsText(base)).toBe("6 columns, a Parts database and 3 labels");
    expect(
      bringsText({
        ...base,
        database_names: [],
        label_names: [],
        task_count: 1,
        columns: [col("To do")],
      }),
    ).toBe("1 column and 1 starter task");
  });

  it("keeps the phone line short", () => {
    expect(compactLine({ ...base, columns: [col("To do"), col("Done")] })).toBe(
      "To do · Done",
    );
    expect(compactLine({ ...base, builtin: false, task_count: 6 })).toBe(
      "6 columns · 6 starter tasks",
    );
  });
});
