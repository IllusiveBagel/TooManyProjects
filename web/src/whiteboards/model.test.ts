import { describe, expect, it } from "vitest";
import { BoardDoc, type ServerElement } from "./doc";
import { copies, deletion, restack } from "./edit";
import {
  arrowEnds,
  CSS_FONTS,
  edge,
  fromData,
  hits,
  resized,
  topAt,
  within,
  wrap,
  type ArrowEl,
  type El,
  type ShapeEl,
} from "./model";

const fonts = CSS_FONTS;
const box = (id: string, x: number, y: number, z = 0): ShapeEl => ({ id, kind: "box", x, y, w: 100, h: 50, fill: "tint", color: "ink", z });
const arrow = (id: string, from?: string, to?: string): ArrowEl => ({ id, kind: "arrow", a: [0, 0], b: [10, 10], from, to, head: true, color: "ink", z: 5 });
const server = (e: El, version: number, deleted = false): ServerElement => {
  const { id, kind, ...data } = e;
  return { id, kind, version, deleted, data: data as unknown as ServerElement["data"], updated_at: "", updated_by_name: null };
};

describe("geometry", () => {
  it("leaves a rect through the side facing the target", () => {
    expect(edge({ x: 0, y: 0, w: 100, h: 50 }, 500, 25, 0)).toEqual([100, 25]);
    expect(edge({ x: 0, y: 0, w: 100, h: 50 }, 50, -500, 0)).toEqual([50, 0]);
  });

  it("joins arrows to the edges of their shapes and falls back to its own ends", () => {
    const all = new Map<string, El>([
      ["a", box("a", 0, 0)],
      ["b", box("b", 300, 0)],
    ]);
    const [p, q] = arrowEnds(arrow("x", "a", "b"), all, fonts);
    expect(p).toEqual([106, 25]);
    expect(q).toEqual([292, 25]);
    expect(arrowEnds(arrow("x", "gone"), all, fonts)).toEqual([
      [0, 0],
      [10, 10],
    ]);
  });

  it("finds the topmost thing under the pointer", () => {
    const els = [box("low", 0, 0, 1), box("high", 50, 0, 2)];
    expect(topAt(els, [60, 10], 0, fonts)?.id).toBe("high");
    expect(topAt(els, [10, 10], 0, fonts)?.id).toBe("low");
    expect(topAt(els, [500, 500], 0, fonts)).toBeUndefined();
  });

  it("only catches the outline of an empty unfilled shape", () => {
    const e: ShapeEl = { ...box("a", 0, 0), fill: "none" };
    const all = new Map<string, El>([["a", e]]);
    expect(hits(e, [50, 25], 4, all, fonts)).toBe(false);
    expect(hits(e, [1, 25], 4, all, fonts)).toBe(true);
  });

  it("selects what is wholly inside a marquee", () => {
    const els = [box("in", 10, 10), box("half", 150, 10)];
    expect(within(els, { x: 0, y: 0, w: 200, h: 100 }, fonts).map((e) => e.id)).toEqual(["in"]);
  });

  it("resizes from a corner without turning inside out", () => {
    expect(resized({ x: 0, y: 0, w: 100, h: 50 }, "se", 20, 10)).toEqual({ x: 0, y: 0, w: 120, h: 60 });
    expect(resized({ x: 0, y: 0, w: 100, h: 50 }, "nw", 200, 0)).toEqual({ x: 84, y: 0, w: 16, h: 50 });
    expect(resized({ x: 0, y: 0, w: 100, h: 50 }, "se", 100, 0, true)).toEqual({ x: 0, y: 0, w: 200, h: 100 });
  });

  it("wraps words to fit and keeps line breaks", () => {
    // 0.56 px per px of size per letter when there is no canvas: 14 * 0.56 ≈ 7.8 px a letter.
    expect(wrap("one two three", 80, "14px x", 14)).toEqual(["one two", "three"]);
    expect(wrap("a\nb", 80, "14px x", 14)).toEqual(["a", "b"]);
  });

  it("reads stored elements back and skips ones it can't draw", () => {
    expect(fromData("a", "box", { x: 1, y: 2, w: 3, h: 4, fill: "none", color: "slot-1", z: 2 })).toMatchObject({ id: "a", kind: "box", x: 1 });
    expect(fromData("a", "box", { x: 1 })).toBeNull();
    expect(fromData("a", "rocket", {})).toBeNull();
    expect(fromData("a", "arrow", { a: [0, 0], b: [1, 1] })).toMatchObject({ head: true, color: "ink", z: 0 });
  });
});

describe("edits", () => {
  const all = new Map<string, El>([
    ["a", box("a", 0, 0)],
    ["b", box("b", 300, 0)],
    ["x", arrow("x", "a", "b")],
  ]);

  it("lets go of arrows when a shape is deleted", () => {
    const c = deletion(all, ["b"], fonts);
    expect(c.b).toBeNull();
    const x = c.x as ArrowEl;
    expect(x.from).toBe("a");
    expect(x.to).toBeUndefined();
    expect(x.b).toEqual([292, 25]);
  });

  it("copies keep arrows joined only to copied shapes", () => {
    const both = copies([all.get("a")!, all.get("b")!, all.get("x")!], all, 10, 10, 100, fonts);
    const x = both[2] as ArrowEl;
    expect(x.from).toBe(both[0]!.id);
    expect(x.to).toBe(both[1]!.id);
    const alone = copies([all.get("x")!], all, 10, 10, 100, fonts)[0] as ArrowEl;
    expect(alone.from).toBeUndefined();
    expect(alone.a).toEqual([116, 35]);
  });

  it("brings things to the front in their order", () => {
    const list = [box("a", 0, 0, 1), box("b", 0, 0, 2), box("c", 0, 0, 3)];
    const c = restack(list, new Set(["a", "b"]), true);
    expect([c.a!.z, c.b!.z]).toEqual([4, 5]);
  });
});

describe("BoardDoc", () => {
  it("records a gesture as one undo step and saves only what changed", () => {
    const d = new BoardDoc();
    d.load([server(box("a", 0, 0), 3)], 7);
    d.change({ b: box("b", 10, 10) });
    const before = { a: d.get("a")! };
    d.live({ a: box("a", 5, 0) });
    d.live({ a: box("a", 9, 0) });
    d.record(before);
    const sent = d.takeChanges();
    expect(sent.map((c) => [c.id, c.version])).toEqual([
      ["b", 1],
      ["a", 4],
    ]);
    d.saved([]);
    expect(d.state).toBe("saved");
    d.undo();
    expect((d.get("a") as ShapeEl).x).toBe(0);
    d.undo();
    expect(d.get("b")).toBeUndefined();
    d.redo();
    expect(d.get("b")).toBeDefined();
    // Deleting still tells the server what kind it was.
    expect(d.takeChanges().find((c) => c.id === "b")).toMatchObject({ kind: "box", version: 3 });
  });

  it("takes in other people's changes but never over yours", () => {
    const d = new BoardDoc();
    d.load([server(box("a", 0, 0), 1), server(box("b", 0, 0), 1)], 1);
    d.change({ a: box("a", 50, 0) });
    d.applyRemote([server(box("a", 99, 0), 2), server(box("b", 77, 0), 2), server(box("c", 1, 1), 1)], 2);
    expect((d.get("a") as ShapeEl).x).toBe(50);
    expect((d.get("b") as ShapeEl).x).toBe(77);
    expect(d.get("c")).toBeDefined();
    expect(d.seq).toBe(2);
    // Ours goes out above theirs, so the later change wins.
    expect(d.takeChanges()[0]).toMatchObject({ id: "a", version: 2 });
    d.saved([]);
    // A remote delete removes it; an old version is ignored.
    d.applyRemote([server(box("b", 0, 0), 3, true), server(box("c", 500, 0), 0)], 3);
    expect(d.get("b")).toBeUndefined();
    expect((d.get("c") as ShapeEl).x).toBe(1);
  });

  it("shows the server's copy when a save loses, and keeps unsaved work after a failure", () => {
    const d = new BoardDoc();
    d.load([server(box("a", 0, 0), 1)], 1);
    d.change({ a: box("a", 5, 0) });
    d.takeChanges();
    d.saved([server(box("a", 40, 0), 9)]);
    expect((d.get("a") as ShapeEl).x).toBe(40);
    d.change({ a: box("a", 41, 0) });
    expect(d.takeChanges()[0]!.version).toBe(10);
    d.failed(false);
    expect(d.state).toBe("unsaved");
    expect(d.takeChanges().map((c) => c.id)).toEqual(["a"]);
  });
});
