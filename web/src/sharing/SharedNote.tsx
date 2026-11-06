import type { Project } from "../api/client";
import { initials } from "./format";

/** A line saying whose project this is and what you can do with it. Nothing for your own. */
export function SharedNote({ project, className = "" }: { project: Project; className?: string }) {
  if (project.role === "owner") return null;
  const editor = project.role === "editor";
  return (
    <p className={`label-mono text-text-quiet flex items-center gap-2 text-[11px] ${className}`}>
      <span
        aria-hidden="true"
        className="bg-surface-raised text-text grid size-6 shrink-0 place-items-center rounded-full border border-current text-[9.5px] font-semibold"
        style={{ borderStyle: editor ? "solid" : "dashed" }}
      >
        {initials(project.owner_name)}
      </span>
      <span>
        Shared by <span className="text-text">{project.owner_name}</span> · {editor ? "you can edit" : "view only"}
      </span>
    </p>
  );
}
