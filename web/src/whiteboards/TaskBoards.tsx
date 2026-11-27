// The whiteboards a task appears on, in its drawer.
import { Link } from "react-router";
import { useTaskWhiteboards } from "./queries";

export function TaskWhiteboards({ taskId }: { taskId: string }) {
  const boards = useTaskWhiteboards(taskId).data ?? [];
  if (!boards.length) return null;
  return (
    <section className="flex flex-col gap-1.5" id="task-whiteboards">
      <h3 className="label-mono text-text-quiet">On whiteboards</h3>
      <ul className="flex flex-col gap-1">
        {boards.map((b) => (
          <li key={b.id}>
            <Link
              to={`/projects/${b.project_id}/whiteboards/${b.id}`}
              className="border-rule hover:bg-surface-raised flex min-h-10 items-center gap-2 rounded-md px-3 text-sm font-semibold"
            >
              {b.title}
            </Link>
          </li>
        ))}
      </ul>
    </section>
  );
}
