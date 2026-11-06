import { Position } from "@xyflow/react";
import { describe, expect, it } from "vitest";
import { borderPoint } from "./floating";

const card = (x: number, y: number) => ({ x, y, w: 200, h: 100 });

describe("borderPoint", () => {
  it("leaves through the side facing the other card", () => {
    expect(borderPoint(card(0, 0), card(400, 0))).toEqual({ x: 200, y: 50, position: Position.Right });
    expect(borderPoint(card(400, 0), card(0, 0))).toEqual({ x: 400, y: 50, position: Position.Left });
    expect(borderPoint(card(0, 0), card(0, 400))).toEqual({ x: 100, y: 100, position: Position.Bottom });
    expect(borderPoint(card(0, 400), card(0, 0)).position).toBe(Position.Top);
  });

  it("uses the top or bottom for steep diagonals", () => {
    expect(borderPoint(card(0, 0), card(100, 500)).position).toBe(Position.Bottom);
    expect(borderPoint(card(0, 0), card(600, 150)).position).toBe(Position.Right);
  });
});
