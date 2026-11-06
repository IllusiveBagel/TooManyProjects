import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";
import type { CSSProperties } from "react";
import type { Project } from "../api/client";
import { projectColor } from "../theme/resolve";
import { dueLabel, lastActive, progress, STATUS_LABEL, STATUS_TONE } from "./format";

export type ProjectNodeData = { project: Project; parentTitle?: string };
export type ProjectNodeType = Node<ProjectNodeData, "project">;

export const CARD_WIDTH = 256;

/** A project card, as in the mockups: colour strip, title, gauge, stats. */
export function ProjectNode({ data, selected }: NodeProps<ProjectNodeType>) {
  const { project, parentTitle } = data;
  const color = projectColor(project.color);
  const { stats } = project;
  const archived = project.status === "archived";
  return (
    <div
      className={`group bg-surface-card text-text relative flex flex-col gap-2.5 rounded-lg border-t-[6px] px-3.5 pt-3 pb-3.5 transition-[translate,box-shadow] ${
        selected ? "border-rule border-accent shadow-card-selected" : "border-rule shadow-card hover:-translate-x-0.5 hover:-translate-y-0.5"
      } ${archived ? "opacity-60" : ""}`}
      style={{ width: CARD_WIDTH, borderTopColor: color }}
    >
      <Handle type="target" position={Position.Left} className="tmp-handle" />
      <div className="flex min-h-5 items-center gap-2">
        {parentTitle && (
          <span className="label-mono truncate font-semibold" style={{ color }} title={`Spawned from ${parentTitle}`}>
            {parentTitle}
          </span>
        )}
        <span className={`label-mono ml-auto shrink-0 rounded-sm border border-current px-1.5 py-0.5 text-[10px] ${STATUS_TONE[project.status] ?? ""}`}>
          {STATUS_LABEL[project.status] ?? project.status}
        </span>
      </div>
      <span className="font-display text-[22px] leading-[1.05] font-extrabold tracking-[-0.02em] break-words">{project.title}</span>
      <div
        className="gauge"
        role="progressbar"
        aria-label="Tasks done"
        aria-valuenow={progress(project)}
        aria-valuemin={0}
        aria-valuemax={100}
        style={{ "--gauge-value": progress(project), "--gauge-color": color } as CSSProperties}
      />
      <div className="label-mono text-text-quiet flex justify-between gap-2">
        <span>
          {stats.done_tasks}/{stats.total_tasks}
        </span>
        {stats.overdue_tasks > 0 ? (
          <span className="text-late">{stats.overdue_tasks} late</span>
        ) : project.target_date ? (
          <span>{dueLabel(project.target_date)}</span>
        ) : (
          <span>No dates</span>
        )}
        <span>{lastActive(stats.last_active_at)}</span>
      </div>
      <Handle type="source" position={Position.Right} className="tmp-handle" />
    </div>
  );
}
