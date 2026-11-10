import { describe, expect, it } from "vitest";
import {
  dueText,
  filterHits,
  findUrl,
  fromShare,
  groupHits,
  hitPath,
  linkTitle,
  matches,
  sessionsHeading,
  shortUrl,
  snippetParts,
  type SearchHit,
} from "./format";

const hit = (kind: string, extra: Partial<SearchHit> = {}): SearchHit => ({
  kind,
  id: `${kind}-1`,
  title: kind,
  done: false,
  updated_at: "2026-11-10T10:00:00Z",
  ...extra,
});

describe("links", () => {
  it("finds the first address and drops trailing punctuation", () => {
    expect(findUrl("Look: https://www.printables.com/model/1-dragon.")).toBe("https://www.printables.com/model/1-dragon");
    expect(findUrl("nothing here")).toBeNull();
  });

  it("shortens for small spaces", () => {
    expect(shortUrl("https://www.printables.com/model/123-dragon")).toBe("printables.com/model/123-dragon");
    expect(shortUrl("https://example.com/a/very/long/path/that/goes/on", 20)).toBe("example.com/a/very/…");
  });

  it("suggests a line from a page's title and site", () => {
    expect(linkTitle({ url: "x", title: "Articulated dragon", site: "Printables" })).toBe("Articulated dragon (Printables)");
    expect(linkTitle({ url: "x", title: "Printables: dragon", site: "Printables" })).toBe("Printables: dragon");
    expect(linkTitle({ url: "x", title: null, site: "Printables" })).toBe("");
  });
});

describe("shares", () => {
  it("splits a share into a line, a link and extra text", () => {
    expect(fromShare({ title: "Dragon", text: "https://p.com/m/1", url: "" })).toEqual({
      text: "Dragon",
      url: "https://p.com/m/1",
      body: "",
    });
    // Android Chrome often sends the link only in text.
    expect(fromShare({ text: "Check this out https://p.com/m/1" })).toEqual({
      text: "Check this out",
      url: "https://p.com/m/1",
      body: "",
    });
    expect(fromShare({ text: "line one\nline two" })).toEqual({ text: "", url: null, body: "line one\nline two" });
  });
});

describe("dates", () => {
  it("says when something is due", () => {
    expect(dueText("2026-11-10", "2026-11-10")).toBe("Today");
    expect(dueText("2026-11-11", "2026-11-10")).toBe("Tomorrow");
    expect(dueText("2026-11-07", "2026-11-10")).toBe("3 days late");
    expect(dueText("2026-11-13", "2026-11-10")).toBe("Fri 13 Nov");
  });

  it("calls an evening of sessions tonight", () => {
    expect(sessionsHeading([new Date(2026, 10, 10, 19)])).toBe("Tonight");
    expect(sessionsHeading([new Date(2026, 10, 10, 13), new Date(2026, 10, 10, 19)])).toBe("Today");
  });
});

describe("palette", () => {
  const hits = [hit("project"), hit("task", { project_id: "p", task_id: "t" }), hit("row", { project_id: "p", database_id: "d" }), hit("note"), hit("comment", { project_id: "p", task_id: "t2" })];

  it("groups tasks, then notes and data, then projects", () => {
    expect(groupHits(hits).map((g) => [g.title, g.hits.map((h) => h.kind)])).toEqual([
      ["Tasks", ["task", "comment"]],
      ["Notes and data", ["row", "note"]],
      ["Projects", ["project"]],
    ]);
  });

  it("filters by kind", () => {
    expect(filterHits(hits, "tasks").map((h) => h.kind)).toEqual(["task", "comment"]);
    expect(filterHits(hits, "all")).toHaveLength(5);
  });

  it("opens each kind in the right place", () => {
    expect(hitPath(hits[1]!)).toBe("/projects/p/board?task=t");
    expect(hitPath(hits[4]!)).toBe("/projects/p/board?task=t2");
    expect(hitPath(hits[2]!)).toBe("/projects/p/data/d?row=row-1");
    expect(hitPath(hits[0]!)).toBe("/projects/project-1");
  });

  it("matches every word", () => {
    expect(matches("plan ti", "Plan time")).toBe(true);
    expect(matches("plan board", "Plan time")).toBe(false);
  });

  it("splits snippets on their hit markers", () => {
    expect(snippetParts("…drain the [fuel] first")).toEqual([
      { text: "…drain the ", hit: false },
      { text: "fuel", hit: true },
      { text: " first", hit: false },
    ]);
  });
});
