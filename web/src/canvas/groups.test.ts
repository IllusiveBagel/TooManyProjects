import { describe, expect, it } from "vitest";
import { between, cardCount, contents, wrap, type Placed } from "./groups";

const card = (id: string, x: number, y: number): Placed => ({ id, x, y, width: 256, height: 140, isGroup: false });
const group = (id: string, x: number, y: number, width: number, height: number): Placed => ({ id, x, y, width, height, isGroup: true });

describe("contents", () => {
  const cars = group("cars", 0, 0, 800, 500);
  it("takes cards whose centre is inside", () => {
    const all = [cars, card("beetle", 40, 80), card("edge", 700, 80), card("away", 1200, 0)];
    // "edge" hangs over the side, but its centre (828) is outside.
    expect(contents(cars, all)).toEqual(["beetle"]);
    expect(cardCount(cars, all)).toBe(1);
  });
  it("takes groups wholly inside, and their cards", () => {
    const videos = group("videos", 400, 200, 380, 280);
    const overlapping = group("overlap", 700, 0, 400, 400);
    const all = [cars, videos, overlapping, card("ep1", 420, 280)];
    expect(contents(cars, all)).toEqual(["videos", "ep1"]);
    expect(contents(videos, all)).toEqual(["ep1"]);
  });
});

describe("wrap and between", () => {
  it("leaves room for the label", () => {
    expect(wrap([card("a", 0, 0), card("b", 400, 300)])).toEqual({ x: -40, y: -72, width: 736, height: 552 });
  });
  it("works whichever way you drag", () => {
    expect(between({ x: 300, y: 50 }, { x: 100, y: 250 })).toEqual({ x: 100, y: 50, width: 200, height: 200 });
  });
});
