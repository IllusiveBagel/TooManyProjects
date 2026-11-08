import type { Task } from "../api/client";
import { dueLabel } from "../canvas/format";
import { initials } from "../sharing/format";
import { projectColor } from "../theme/resolve";
import { PRIORITY_LABEL, taskRef } from "./format";

/** A card on a board. `showProject` adds the project's colour strip and name. */
export function TaskCard({
  task,
  onOpen,
  showProject = false,
  lifted = false,
}: {
  task: Task;
  onOpen: (id: string) => void;
  showProject?: boolean;
  lifted?: boolean;
}) {
  const done = task.category === "done";
  const hot = task.priority === "urgent" || task.priority === "high";
  return (
    <div
      role="button"
      tabIndex={-1}
      onClick={() => onOpen(task.id)}
      onKeyDown={(e) => e.key === "Enter" && onOpen(task.id)}
      className={`border-rule bg-surface-card flex cursor-grab flex-col gap-2 rounded-md p-3 text-left ${lifted ? "shadow-card-selected rotate-1" : "shadow-card"} ${done ? "opacity-75" : ""}`}
      style={showProject ? { borderLeftWidth: 6, borderLeftColor: projectColor(task.project_color) } : undefined}
    >
      {showProject && <span className="label-mono text-text-quiet truncate text-[10px]">{task.project_title}</span>}
      <span className={`font-bold leading-snug ${done ? "line-through decoration-1" : ""}`}>{task.title}</span>
      {task.labels.length > 0 && (
        <span className="flex flex-wrap gap-1">
          {task.labels.map((l) => (
            <span
              key={l.id}
              className="label-mono rounded-sm px-1.5 py-0.5 text-[10px]"
              style={{
                background: `color-mix(in srgb, ${l.color ? projectColor(l.color) : "var(--tmp-text-quiet)"} 22%, transparent)`,
              }}
            >
              {l.name}
            </span>
          ))}
        </span>
      )}
      <span className="label-mono text-text-quiet flex items-center gap-3 text-[10px]">
        <span>{taskRef(task)}</span>
        {task.priority && <span className={hot ? "text-late" : ""}>{PRIORITY_LABEL[task.priority]}</span>}
        {task.checklist_total > 0 && (
          <span className={task.checklist_done === task.checklist_total ? "text-done" : ""}>
            ☑ {task.checklist_done}/{task.checklist_total}
          </span>
        )}
        {task.comment_count > 0 && (
          <span title={`${task.comment_count} comment${task.comment_count === 1 ? "" : "s"} in the log`}>✎ {task.comment_count}</span>
        )}
        {task.note_count > 0 && (
          <span title={`${task.note_count} linked note${task.note_count === 1 ? "" : "s"}`}>¶ {task.note_count}</span>
        )}
        {task.due_date && !done && (
          <span className={`ml-auto ${task.overdue ? "text-late font-semibold" : ""}`}>{dueLabel(task.due_date)}</span>
        )}
        {task.assignee_name && (
          <span
            title={`Assigned to ${task.assignee_name}`}
            className={`bg-surface-raised text-text grid size-6 shrink-0 place-items-center rounded-full text-[9px] font-semibold ${task.due_date && !done ? "" : "ml-auto"}`}
          >
            {initials(task.assignee_name)}
          </span>
        )}
      </span>
    </div>
  );
}
