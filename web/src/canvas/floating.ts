import { Position, type InternalNode } from "@xyflow/react";

type Box = { x: number; y: number; w: number; h: number };

function box(node: InternalNode): Box {
  const { x, y } = node.internals.positionAbsolute;
  return { x, y, w: node.measured.width ?? 0, h: node.measured.height ?? 0 };
}

/**
 * Where the straight line between two boxes' centres leaves `from`, and which
 * side that is. Links attach to the nearest side instead of fixed handles, so
 * arrows stay short however the cards are arranged.
 */
export function borderPoint(from: Box, to: Box): { x: number; y: number; position: Position } {
  const cx = from.x + from.w / 2;
  const cy = from.y + from.h / 2;
  const dx = to.x + to.w / 2 - cx;
  const dy = to.y + to.h / 2 - cy;
  if (dx === 0 && dy === 0) return { x: cx, y: from.y, position: Position.Top };
  // Scale the direction so it just touches the box edge.
  const sx = dx === 0 ? Infinity : from.w / 2 / Math.abs(dx);
  const sy = dy === 0 ? Infinity : from.h / 2 / Math.abs(dy);
  if (sx < sy) {
    return { x: cx + dx * sx, y: cy + dy * sx, position: dx > 0 ? Position.Right : Position.Left };
  }
  return { x: cx + dx * sy, y: cy + dy * sy, position: dy > 0 ? Position.Bottom : Position.Top };
}

export function floatingEdgeParams(source: InternalNode, target: InternalNode) {
  const s = box(source);
  const t = box(target);
  const a = borderPoint(s, t);
  const b = borderPoint(t, s);
  return {
    sourceX: a.x,
    sourceY: a.y,
    sourcePosition: a.position,
    targetX: b.x,
    targetY: b.y,
    targetPosition: b.position,
  };
}
