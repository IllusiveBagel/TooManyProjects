import type { ProjectTemplate } from "../api/client";
import { CATEGORY_COLOR } from "../tasks/format";
import { compactLine } from "./format";

/** The column names as small chips, coloured by what each counts as. */
function Chips({ template }: { template: ProjectTemplate }) {
  return (
    <span className="flex flex-wrap gap-1">
      {template.columns.map((c, i) => (
        <span
          key={i}
          className="label-mono border-rule rounded-sm px-1.5 py-0.5 text-[10.5px]"
          style={{
            color:
              c.category === "todo" || c.category === "done"
                ? undefined
                : CATEGORY_COLOR[c.category],
          }}
        >
          {c.name}
        </span>
      ))}
    </span>
  );
}

/** "Start from": a grid of cards on wide screens, a radio list on phones. */
export function TemplatePicker({
  templates,
  value,
  onChange,
}: {
  templates: ProjectTemplate[];
  value: string;
  onChange: (id: string) => void;
}) {
  return (
    <fieldset className="flex min-w-0 flex-col gap-2">
      <legend className="label-mono text-text-quiet mb-2">Start from</legend>
      <div className="grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3 sm:gap-3">
        {templates.map((t) => {
          const on = t.id === value;
          return (
            <label
              key={t.id}
              className={`border-rule flex cursor-pointer items-center gap-3 rounded-md px-3.5 py-3 sm:flex-col sm:items-stretch sm:gap-2.5 sm:p-4 ${
                on
                  ? "bg-surface-raised shadow-button !border-accent"
                  : "hover:bg-surface-raised"
              }`}
            >
              <input
                type="radio"
                name="template"
                className="sr-only"
                checked={on}
                onChange={() => onChange(t.id)}
              />
              <span
                aria-hidden="true"
                className={`grid size-6 shrink-0 place-items-center rounded-full border-2 sm:hidden ${on ? "border-accent" : "border-line"}`}
              >
                {on && <span className="bg-accent size-3 rounded-full" />}
              </span>
              <span className="flex min-w-0 flex-col gap-1 sm:gap-2.5">
                <span className="flex items-baseline gap-2">
                  <span className="font-display text-[17px] leading-tight font-extrabold sm:text-[19px]">
                    {t.name}
                    {!t.builtin && (
                      <span className="sm:hidden">
                        {" "}
                        · {t.mine ? "yours" : t.owner_name}
                      </span>
                    )}
                  </span>
                  {!t.builtin && (
                    <span className="label-mono text-text-quiet ml-auto shrink-0 text-[10px] max-sm:hidden">
                      {t.mine ? "Yours" : t.owner_name}
                    </span>
                  )}
                </span>
                <span className="label-mono text-text-quiet text-[11px] leading-relaxed uppercase sm:hidden">
                  {compactLine(t)}
                </span>
                <span className="max-sm:hidden">
                  <Chips template={t} />
                </span>
                <span className="text-text-quiet text-sm leading-snug max-sm:hidden">
                  {t.blurb}
                </span>
              </span>
            </label>
          );
        })}
      </div>
    </fieldset>
  );
}
