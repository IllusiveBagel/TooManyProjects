// Whiteboard elements and the geometry the editor needs: bounds, where arrows
// meet shapes, and what is under the pointer. Pure functions, no React.

export type Slot = `slot-${1 | 2 | 3 | 4 | 5 | 6 | 7 | 8}` | "ink";
export type Fill = "none" | "tint" | "solid";
export type Pt = [number, number];

export const SLOTS: Slot[] = ["ink", "slot-1", "slot-2", "slot-3", "slot-4", "slot-5", "slot-6", "slot-7", "slot-8"];

interface Base {
  id: string;
  /** Stacking order: higher draws on top. */
  z: number;
  color: Slot;
}
export interface ShapeEl extends Base {
  kind: "box" | "ellipse" | "diamond";
  x: number;
  y: number;
  w: number;
  h: number;
  fill: Fill;
  text?: string;
  /** The id of a task this shape stands for. */
  task?: string;
  /** That task's number, shown as a chip (such as `T-14`). */
  taskRef?: string;
}
export interface StickyEl extends Base {
  kind: "sticky";
  x: number;
  y: number;
  w: number;
  h: number;
  text: string;
}
export type TextSize = "s" | "m" | "l" | "xl";
export interface TextEl extends Base {
  kind: "text";
  x: number;
  y: number;
  text: string;
  size: TextSize;
  /** Written in the reading face, like a note in the margin. */
  hand?: boolean;
}
export interface ArrowEl extends Base {
  kind: "arrow";
  /** The element each end is joined to, if any. A joined end follows it. */
  from?: string;
  to?: string;
  /** Where each end is when it isn't joined (or the element has gone). */
  a: Pt;
  b: Pt;
  label?: string;
  dashed?: boolean;
  /** False for a plain line. */
  head: boolean;
}
export interface PenEl extends Base {
  kind: "pen";
  points: Pt[];
  width: number;
}
export interface ImageEl extends Base {
  kind: "image";
  x: number;
  y: number;
  w: number;
  h: number;
  /** The project photo (attachment) shown. */
  file: string;
}
export type El = ShapeEl | StickyEl | TextEl | ArrowEl | PenEl | ImageEl;
export type Kind = El["kind"];
export type Boxed = ShapeEl | StickyEl | ImageEl;

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export const TEXT_SIZE: Record<TextSize, number> = { s: 14, m: 18, l: 24, xl: 34 };
export const LINE = 1.2;

/** Elements with a box you can resize from its corners. */
export const isBoxed = (e: El): e is Boxed =>
  e.kind === "box" || e.kind === "ellipse" || e.kind === "diamond" || e.kind === "sticky" || e.kind === "image";
export const isShape = (e: El): e is ShapeEl => e.kind === "box" || e.kind === "ellipse" || e.kind === "diamond";
/** Elements an arrow can join. */
export const canJoin = (e: El) => isBoxed(e) || e.kind === "text";

// ---------------------------------------------------------------------------
// Text measuring. In the browser it uses a canvas with the real fonts; in
// tests it estimates.

let ctx: CanvasRenderingContext2D | null | undefined;
function measureCtx() {
  if (ctx === undefined) {
    try {
      ctx = typeof document === "undefined" ? null : document.createElement("canvas").getContext("2d");
    } catch {
      ctx = null;
    }
  }
  return ctx;
}

/** Width of one line of text in `font` (a CSS font shorthand), in px. */
export function measure(text: string, font: string, size: number): number {
  const c = measureCtx();
  if (!c) return text.length * size * 0.56;
  c.font = font;
  return c.measureText(text).width;
}

/** Breaks text into lines no wider than `max`, keeping the writer's own line breaks. */
export function wrap(text: string, max: number, font: string, size: number): string[] {
  const out: string[] = [];
  for (const para of text.split("\n")) {
    const words = para.split(/(\s+)/);
    let line = "";
    for (const w of words) {
      const next = line + w;
      if (line && measure(next.trimEnd(), font, size) > max) {
        out.push(line.trimEnd());
        line = w.trimStart();
        // One word longer than the line: break it.
        while (line && measure(line, font, size) > max) {
          let n = line.length - 1;
          while (n > 1 && measure(line.slice(0, n), font, size) > max) n--;
          out.push(line.slice(0, n));
          line = line.slice(n);
        }
      } else line = next;
    }
    out.push(line.trimEnd());
  }
  return out;
}

/** The CSS font for each kind of writing. Families come from the theme. */
export interface Fonts {
  shape: string;
  sticky: string;
  text: (e: Pick<TextEl, "size" | "hand">) => string;
  label: string;
}

export function fontsFrom(family: (name: "interface" | "display" | "reading" | "mono") => string): Fonts {
  return {
    shape: `600 17px ${family("interface")}`,
    sticky: `500 16px ${family("reading")}`,
    text: (e) =>
      e.hand ? `500 ${TEXT_SIZE[e.size]}px ${family("reading")}` : `800 ${TEXT_SIZE[e.size]}px ${family("display")}`,
    label: `600 12px ${family("mono")}`,
  };
}

/** Fonts as named in the page's stylesheet, for drawing on screen. */
export const CSS_FONTS = fontsFrom((n) => `var(--tmp-font-${n})`);

/** Fonts with the theme's real family names, for measuring on a canvas. */
export function resolvedFonts(): Fonts {
  if (typeof document === "undefined") return CSS_FONTS;
  const s = getComputedStyle(document.documentElement);
  return fontsFrom((n) => s.getPropertyValue(`--tmp-font-${n}`).trim() || "sans-serif");
}

export function textBox(e: TextEl, fonts: Fonts): Rect {
  const size = TEXT_SIZE[e.size];
  const lines = (e.text || " ").split("\n");
  const w = Math.max(size * 0.6, ...lines.map((l) => measure(l, fonts.text(e), size)));
  return { x: e.x, y: e.y, w, h: lines.length * size * LINE };
}

// ---------------------------------------------------------------------------
// Geometry

/** The box an element sits in, for anything but arrows (which follow their ends). */
export function rectOf(e: El, fonts: Fonts, all?: Map<string, El>): Rect {
  switch (e.kind) {
    case "text":
      return textBox(e, fonts);
    case "pen": {
      const r = pointsRect(e.points);
      return { x: r.x - e.width, y: r.y - e.width, w: r.w + e.width * 2, h: r.h + e.width * 2 };
    }
    case "arrow": {
      const [a, b] = arrowEnds(e, all ?? new Map(), fonts);
      return pointsRect([a, b]);
    }
    default:
      return { x: e.x, y: e.y, w: e.w, h: e.h };
  }
}

export function pointsRect(points: Pt[]): Rect {
  let x0 = Infinity,
    y0 = Infinity,
    x1 = -Infinity,
    y1 = -Infinity;
  for (const [x, y] of points) {
    x0 = Math.min(x0, x);
    y0 = Math.min(y0, y);
    x1 = Math.max(x1, x);
    y1 = Math.max(y1, y);
  }
  if (x0 === Infinity) return { x: 0, y: 0, w: 0, h: 0 };
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

export function union(rects: Rect[]): Rect | null {
  if (!rects.length) return null;
  const pts: Pt[] = rects.flatMap((r) => [
    [r.x, r.y],
    [r.x + r.w, r.y + r.h],
  ]);
  return pointsRect(pts);
}

/** Everything on the board, or nothing. */
export function bounds(els: El[], fonts: Fonts): Rect | null {
  const all = new Map(els.map((e) => [e.id, e]));
  return union(els.map((e) => rectOf(e, fonts, all)));
}

/** Where the line from a rect's centre towards (tx, ty) leaves it, `pad` px out. */
export function edge(r: Rect, tx: number, ty: number, pad = 6): Pt {
  const cx = r.x + r.w / 2,
    cy = r.y + r.h / 2;
  const dx = tx - cx,
    dy = ty - cy;
  if (!dx && !dy) return [cx, cy];
  const sx = dx ? (r.w / 2 + pad) / Math.abs(dx) : Infinity;
  const sy = dy ? (r.h / 2 + pad) / Math.abs(dy) : Infinity;
  const s = Math.min(sx, sy);
  return [cx + dx * s, cy + dy * s];
}

const centre = (r: Rect): Pt => [r.x + r.w / 2, r.y + r.h / 2];

/** An arrow's two ends: on the edge of what each end is joined to, else where it was left. */
export function arrowEnds(e: ArrowEl, all: Map<string, El>, fonts: Fonts): [Pt, Pt] {
  const from = e.from ? all.get(e.from) : undefined;
  const to = e.to ? all.get(e.to) : undefined;
  const fr = from && from.kind !== "arrow" ? rectOf(from, fonts) : null;
  const tr = to && to.kind !== "arrow" ? rectOf(to, fonts) : null;
  const aim = (r: Rect | null, p: Pt): Pt => (r ? centre(r) : p);
  const a = fr ? edge(fr, ...aim(tr, e.b), 6) : e.a;
  const b = tr ? edge(tr, ...aim(fr, e.a), 8) : e.b;
  return [a, b];
}

function distToSegment(p: Pt, a: Pt, b: Pt): number {
  const [px, py] = p,
    [ax, ay] = a,
    [bx, by] = b;
  const dx = bx - ax,
    dy = by - ay;
  const len = dx * dx + dy * dy;
  const t = len ? Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / len)) : 0;
  return Math.hypot(px - (ax + t * dx), py - (ay + t * dy));
}

const inside = (p: Pt, r: Rect, pad = 0) =>
  p[0] >= r.x - pad && p[0] <= r.x + r.w + pad && p[1] >= r.y - pad && p[1] <= r.y + r.h + pad;

/** Whether `p` touches the element, `tol` px of slack (in board units). */
export function hits(e: El, p: Pt, tol: number, all: Map<string, El>, fonts: Fonts): boolean {
  switch (e.kind) {
    case "arrow": {
      const [a, b] = arrowEnds(e, all, fonts);
      return distToSegment(p, a, b) <= tol + 2;
    }
    case "pen": {
      const pts = e.points;
      if (pts.length === 1) return Math.hypot(p[0] - pts[0]![0], p[1] - pts[0]![1]) <= tol + e.width;
      for (let i = 1; i < pts.length; i++) if (distToSegment(p, pts[i - 1]!, pts[i]!) <= tol + e.width) return true;
      return false;
    }
    case "box":
    case "ellipse":
    case "diamond":
      // A shape with no fill is only its outline and its words, so you can
      // reach what's behind it.
      if (e.fill === "none" && !e.text) {
        const r = rectOf(e, fonts);
        return inside(p, r, tol) && !inside(p, { x: r.x + tol, y: r.y + tol, w: r.w - 2 * tol, h: r.h - 2 * tol });
      }
      return inside(p, rectOf(e, fonts), tol);
    default:
      return inside(p, rectOf(e, fonts), tol);
  }
}

/** Topmost element under `p`. */
export function topAt(els: El[], p: Pt, tol: number, fonts: Fonts, only?: (e: El) => boolean): El | undefined {
  const all = new Map(els.map((e) => [e.id, e]));
  for (let i = els.length - 1; i >= 0; i--) {
    const e = els[i]!;
    if (only && !only(e)) continue;
    if (hits(e, p, tol, all, fonts)) return e;
  }
  return undefined;
}

/** Elements wholly inside a marquee. */
export function within(els: El[], r: Rect, fonts: Fonts): El[] {
  const all = new Map(els.map((e) => [e.id, e]));
  return els.filter((e) => {
    const b = rectOf(e, fonts, all);
    return b.x >= r.x && b.y >= r.y && b.x + b.w <= r.x + r.w && b.y + b.h <= r.y + r.h;
  });
}

/** The rect two corners make, whichever way round. */
export function fromCorners(a: Pt, b: Pt): Rect {
  return { x: Math.min(a[0], b[0]), y: Math.min(a[1], b[1]), w: Math.abs(b[0] - a[0]), h: Math.abs(b[1] - a[1]) };
}

/** Moves an element by (dx, dy). Joined arrow ends follow their shapes, not this. */
export function moved<T extends El>(e: T, dx: number, dy: number): T {
  switch (e.kind) {
    case "pen":
      return { ...e, points: e.points.map(([x, y]) => [x + dx, y + dy] as Pt) };
    case "arrow":
      return { ...e, a: [e.a[0] + dx, e.a[1] + dy], b: [e.b[0] + dx, e.b[1] + dy] };
    default:
      return { ...e, x: (e as Boxed).x + dx, y: (e as Boxed).y + dy };
  }
}

export type Handle = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";
export const HANDLES: Handle[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];

export function handlePoint(r: Rect, h: Handle, pad: number): Pt {
  const x0 = r.x - pad,
    y0 = r.y - pad,
    x1 = r.x + r.w + pad,
    y1 = r.y + r.h + pad;
  const x = h.includes("w") ? x0 : h.includes("e") ? x1 : (x0 + x1) / 2;
  const y = h.includes("n") ? y0 : h.includes("s") ? y1 : (y0 + y1) / 2;
  return [x, y];
}

/** The rect after dragging handle `h` of `r` by (dx, dy); `keep` holds the shape's proportions. */
export function resized(r: Rect, h: Handle, dx: number, dy: number, keep = false, min = 16): Rect {
  let { x, y, w, h: hh } = r;
  if (h.includes("w")) {
    x += dx;
    w -= dx;
  }
  if (h.includes("e")) w += dx;
  if (h.includes("n")) {
    y += dy;
    hh -= dy;
  }
  if (h.includes("s")) hh += dy;
  if (keep && r.w && r.h && h.length === 2) {
    const s = Math.max(w / r.w, hh / r.h);
    const nw = r.w * s,
      nh = r.h * s;
    if (h.includes("w")) x = r.x + r.w - nw;
    if (h.includes("n")) y = r.y + r.h - nh;
    w = nw;
    hh = nh;
  }
  if (w < min) {
    if (h.includes("w")) x -= min - w;
    w = min;
  }
  if (hh < min) {
    if (h.includes("n")) y -= min - hh;
    hh = min;
  }
  return { x, y, w, h: hh };
}

/** Sorted by stacking order, then id so everyone draws the same order. */
export function sortByZ(els: Iterable<El>): El[] {
  return [...els].sort((a, b) => a.z - b.z || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
}

/** A fresh id for an element: short, random and safe for the server. */
export function newId(): string {
  const bytes = new Uint8Array(9);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(36).padStart(2, "0")).join("").slice(0, 14);
}

/** Turns stored data back into an element, or null when it isn't one we can draw. */
export function fromData(id: string, kind: string, data: Record<string, unknown>): El | null {
  const e = { ...data, id, kind } as El;
  const num = (v: unknown) => typeof v === "number" && Number.isFinite(v);
  const pt = (v: unknown) => Array.isArray(v) && v.length === 2 && num(v[0]) && num(v[1]);
  if (!num(e.z)) (e as { z: number }).z = 0;
  if (typeof e.color !== "string") (e as { color: Slot }).color = "ink";
  switch (e.kind) {
    case "box":
    case "ellipse":
    case "diamond":
    case "sticky":
    case "image":
      return num(e.x) && num(e.y) && num(e.w) && num(e.h) ? e : null;
    case "text":
      return num(e.x) && num(e.y) && typeof e.text === "string" ? { ...e, size: e.size ?? "m" } : null;
    case "arrow":
      return pt(e.a) && pt(e.b) ? { ...e, head: e.head !== false } : null;
    case "pen":
      return Array.isArray(e.points) && e.points.every(pt) ? { ...e, width: num(e.width) ? e.width : 3 } : null;
    default:
      return null;
  }
}

/** What the server stores for an element: everything but its id and kind. */
export function toData(e: El): Record<string, unknown> {
  const { id: _id, kind: _kind, ...rest } = e;
  return rest;
}

/** Where a shape's task chip sits, in board units. */
export const taskChip = (e: Pick<ShapeEl, "x" | "y" | "w">): Rect => ({ x: e.x + e.w - 50, y: e.y - 11, w: 48, h: 20 });
