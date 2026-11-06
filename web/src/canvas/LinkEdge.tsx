import { BaseEdge, EdgeLabelRenderer, getBezierPath, useInternalNode, type Edge, type EdgeProps } from "@xyflow/react";
import { floatingEdgeParams } from "./floating";
import type { Link } from "../api/client";
import { useDeleteLink, useUpdateLink } from "./queries";

export type LinkEdgeData = { link: Link; color: string };
export type LinkEdgeType = Edge<LinkEdgeData, "link">;

const KIND_LABEL: Record<string, string> = { spawned: "Spawned", related: "Related", blocks: "Blocks" };

/** Spawned: thick, in the parent's colour. Related: dashed and quiet. Blocks: dashed, in the late colour. */
export function edgeStyle(kind: string, color: string) {
  switch (kind) {
    case "spawned":
      return { stroke: color, strokeWidth: "var(--tmp-link-width)" };
    case "blocks":
      return { stroke: "var(--tmp-late)", strokeWidth: 2, strokeDasharray: "7 6" };
    default:
      return { stroke: "var(--tmp-text-quiet)", strokeWidth: 2, strokeDasharray: "7 6" };
  }
}

export function LinkEdge(props: EdgeProps<LinkEdgeType>) {
  const { id, data, selected, markerEnd, source, target } = props;
  const sourceNode = useInternalNode(source);
  const targetNode = useInternalNode(target);
  const update = useUpdateLink();
  const remove = useDeleteLink();
  if (!data || !sourceNode || !targetNode) return null;
  const [path, labelX, labelY] = getBezierPath(floatingEdgeParams(sourceNode, targetNode));
  const { link, color } = data;
  const tone = link.kind === "spawned" ? color : link.kind === "blocks" ? "var(--tmp-late)" : "var(--tmp-text-quiet)";
  const text = link.label ?? (link.kind === "related" ? null : KIND_LABEL[link.kind]);

  return (
    <>
      <BaseEdge id={id} path={path} markerEnd={markerEnd} style={edgeStyle(link.kind, color)} interactionWidth={24} />
      <EdgeLabelRenderer>
        <div
          className="nodrag nopan pointer-events-auto absolute flex flex-col items-center gap-1.5"
          style={{ transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)` }}
        >
          {text && (
            <span
              className="label-mono bg-surface rounded-sm border-[1.5px] px-1.5 py-0.5 text-[10px] tracking-[0.1em]"
              style={{ color: tone, borderColor: tone }}
            >
              {text}
            </span>
          )}
          {selected && (
            <div role="toolbar" aria-label="Link" className="border-rule bg-surface-card shadow-card flex items-center gap-0.5 rounded-md p-1">
              {(["spawned", "related", "blocks"] as const).map((k) => (
                <button
                  key={k}
                  type="button"
                  aria-pressed={link.kind === k}
                  onClick={() => update.mutate({ id: link.id, kind: k })}
                  className={`label-mono min-h-9 cursor-pointer rounded-sm px-2 ${link.kind === k ? "bg-accent text-on-accent" : "text-text-quiet hover:text-text"}`}
                >
                  {KIND_LABEL[k]}
                </button>
              ))}
              <button
                type="button"
                onClick={() => update.mutate({ id: link.id, reverse: true })}
                className="label-mono text-text-quiet hover:text-text min-h-9 cursor-pointer px-2"
                title="Swap direction"
              >
                ⇄
              </button>
              <button
                type="button"
                onClick={() => remove.mutate(link.id)}
                className="label-mono text-late min-h-9 cursor-pointer px-2"
              >
                Remove
              </button>
            </div>
          )}
          {selected && (update.error || remove.error) && (
            <span role="alert" className="bg-surface-card text-late border-rule max-w-[240px] rounded-sm px-2 py-1 text-xs">
              {(update.error ?? remove.error)?.message}
            </span>
          )}
        </div>
      </EdgeLabelRenderer>
    </>
  );
}
