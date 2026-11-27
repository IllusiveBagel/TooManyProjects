// Edits that touch more than one element: deleting a shape lets go of the
// arrows joined to it, and copies keep arrows joined only to what was copied.
import type { Changes } from "./doc";
import { arrowEnds, moved, newId, type ArrowEl, type El, type Fonts } from "./model";

/** Arrows joined to any of `ids` let go of them, staying where they are drawn now. */
export function detachArrows(all: Map<string, El>, ids: Set<string>, fonts: Fonts): Changes {
  const out: Changes = {};
  for (const e of all.values()) {
    if (e.kind !== "arrow" || ids.has(e.id)) continue;
    const from = e.from && ids.has(e.from);
    const to = e.to && ids.has(e.to);
    if (!from && !to) continue;
    const [a, b] = arrowEnds(e, all, fonts);
    const next: ArrowEl = { ...e, a, b };
    if (from) delete next.from;
    if (to) delete next.to;
    out[e.id] = next;
  }
  return out;
}

/** Deletes `ids`, letting go of arrows joined to them. */
export function deletion(all: Map<string, El>, ids: Iterable<string>, fonts: Fonts): Changes {
  const gone = new Set(ids);
  const out = detachArrows(all, gone, fonts);
  for (const id of gone) if (all.has(id)) out[id] = null;
  return out;
}

/**
 * Copies of `els` moved by (dx, dy), with new ids, stacked from `z`. An arrow
 * stays joined to an element only if that element was copied too.
 */
export function copies(els: El[], all: Map<string, El>, dx: number, dy: number, z: number, fonts: Fonts): El[] {
  const ids = new Map(els.map((e) => [e.id, newId()]));
  return els.map((e, i) => {
    let c: El = { ...moved(e, dx, dy), id: ids.get(e.id)!, z: z + i };
    if (c.kind === "arrow" && e.kind === "arrow") {
      const [a, b] = arrowEnds(e, all, fonts);
      const arrow: ArrowEl = { ...c, a: [a[0] + dx, a[1] + dy], b: [b[0] + dx, b[1] + dy] };
      delete arrow.from;
      delete arrow.to;
      if (e.from && ids.has(e.from)) arrow.from = ids.get(e.from);
      if (e.to && ids.has(e.to)) arrow.to = ids.get(e.to);
      c = arrow;
    }
    return c;
  });
}

/** Changes that bring `ids` to the top (or bottom) of the stack, keeping their order. */
export function restack(list: El[], ids: Set<string>, top: boolean): Changes {
  const picked = list.filter((e) => ids.has(e.id));
  if (!picked.length) return {};
  const zs = list.map((e) => e.z);
  const base = top ? Math.max(...zs) + 1 : Math.min(...zs) - picked.length;
  const out: Changes = {};
  picked.forEach((e, i) => (out[e.id] = { ...e, z: base + i }));
  return out;
}
