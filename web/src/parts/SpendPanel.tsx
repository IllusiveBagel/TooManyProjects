import { useOpenTask } from "../tasks/TaskDrawer";
import { money, plural, waitingText } from "./format";
import { useProjectSpend } from "./queries";

/** Spend on a project's parts, its family's, and the tasks waiting on parts.
 * Shows nothing for a project with no parts. */
export function SpendPanel({ projectId, className = "" }: { projectId: string; className?: string }) {
  const spend = useProjectSpend(projectId).data;
  const [, openTask] = useOpenTask();
  if (!spend || (spend.part_count === 0 && !spend.family)) return null;
  const c = spend.currency;
  const all = spend.spent + spend.to_buy;
  const share = all > 0 ? (spend.spent / all) * 100 : 0;
  return (
    <div className={`flex flex-col gap-4 ${className}`}>
      <section className="border-rule bg-surface-card shadow-card rounded-lg p-4 sm:p-5">
        <h2 className="label-mono text-text-quiet">Spend</h2>
        <p className="font-display mt-1 text-[40px] leading-tight font-extrabold tracking-[-0.03em]">{money(spend.spent, c)}</p>
        <p className="text-text-quiet">spent so far (ordered, shipped, arrived, fitted)</p>
        <div
          className="bg-surface-raised mt-3 h-3 overflow-hidden rounded-[3px]"
          role="img"
          aria-label={`${money(spend.spent, c)} spent of ${money(all, c)}`}
        >
          <div className="bg-accent h-full" style={{ width: `${share}%` }} />
        </div>
        <p className="label-mono mt-2.5 text-[12px] tracking-[0.08em]">
          {money(spend.to_buy, c)} still to buy · {plural(spend.need_count, "part")}
        </p>
        {spend.family && (
          <p className="label-mono text-text-quiet mt-2 text-[11px] leading-relaxed tracking-[0.08em]">
            Family incl. {spend.family.projects.join(", ")}: {money(spend.family.spent, c)}
            {spend.family.to_buy > 0 && ` spent, ${money(spend.family.to_buy, c)} to buy`}
          </p>
        )}
      </section>
      {spend.waiting.length > 0 && (
        <section className="border-rule bg-surface-card shadow-card rounded-lg p-4 sm:p-5">
          <h2 className="label-mono text-text-quiet mb-1">Waiting on parts</h2>
          <ul className="flex flex-col">
            {spend.waiting.map((w) => (
              <li key={w.id}>
                <button type="button" onClick={() => openTask(w.id)} className="flex w-full cursor-pointer flex-col items-start py-2 text-left">
                  <span className="font-bold hover:underline">
                    T-{w.number} {w.title}
                  </span>
                  <span
                    className="label-mono text-[11px] tracking-[0.08em]"
                    style={{ color: w.need > 0 ? "var(--tmp-slot-6)" : "var(--tmp-slot-3)" }}
                  >
                    {waitingText(w)}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
