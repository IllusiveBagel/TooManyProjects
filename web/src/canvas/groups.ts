/** Geometry for canvas groups: which cards sit inside an area, and the area around a selection. */

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Placed extends Rect {
  id: string;
  isGroup: boolean;
}

export const MIN_GROUP = 120;
/** Room for the label tab above the first row of cards. */
const WRAP_PAD = 40;
const WRAP_TOP = 72;

const centreInside = (r: Rect, g: Rect) => {
  const cx = r.x + r.width / 2;
  const cy = r.y + r.height / 2;
  return cx >= g.x && cx <= g.x + g.width && cy >= g.y && cy <= g.y + g.height;
};

const wholeInside = (r: Rect, g: Rect) =>
  r.x >= g.x && r.y >= g.y && r.x + r.width <= g.x + g.width && r.y + r.height <= g.y + g.height;

/**
 * Everything a group carries when it moves: cards whose centre is inside it,
 * and smaller groups that sit wholly inside it (with their cards, which are
 * inside this one too).
 */
export function contents(group: Placed, all: Placed[]): string[] {
  return all
    .filter((n) => n.id !== group.id && (n.isGroup ? wholeInside(n, group) : centreInside(n, group)))
    .map((n) => n.id);
}

/** How many cards are inside. */
export function cardCount(group: Rect, all: Placed[]): number {
  return all.filter((n) => !n.isGroup && centreInside(n, group)).length;
}

/** A group drawn around some cards, with room for its label. */
export function wrap(rects: Rect[]): Rect {
  const x = Math.min(...rects.map((r) => r.x)) - WRAP_PAD;
  const y = Math.min(...rects.map((r) => r.y)) - WRAP_TOP;
  const right = Math.max(...rects.map((r) => r.x + r.width)) + WRAP_PAD;
  const bottom = Math.max(...rects.map((r) => r.y + r.height)) + WRAP_PAD;
  return { x: Math.round(x), y: Math.round(y), width: Math.round(right - x), height: Math.round(bottom - y) };
}

/** The rectangle between two corners, any way round. */
export function between(a: { x: number; y: number }, b: { x: number; y: number }): Rect {
  return {
    x: Math.round(Math.min(a.x, b.x)),
    y: Math.round(Math.min(a.y, b.y)),
    width: Math.round(Math.abs(a.x - b.x)),
    height: Math.round(Math.abs(a.y - b.y)),
  };
}
