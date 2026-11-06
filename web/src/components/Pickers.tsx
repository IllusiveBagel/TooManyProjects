import { projectColor } from "../theme/resolve";
import { STATUS_LABEL, STATUS_ORDER } from "../canvas/format";

/** Eight theme slots as swatches. */
export function ColorPicker({ value, onChange }: { value: string; onChange: (color: string) => void }) {
  return (
    <fieldset className="flex flex-col gap-1.5">
      <legend className="label-mono text-text-quiet mb-1.5">Colour</legend>
      <div className="flex flex-wrap gap-1">
        {Array.from({ length: 8 }, (_, i) => {
          const slot = `slot-${i + 1}`;
          const on = value === slot;
          return (
            <label
              key={slot}
              className={`grid size-10 cursor-pointer place-items-center rounded-md border-2 ${on ? "border-text" : "border-transparent"}`}
            >
              <input type="radio" name="color" className="sr-only" checked={on} onChange={() => onChange(slot)} aria-label={`Colour ${i + 1}`} />
              <span className="size-6 rounded-sm" style={{ background: projectColor(slot) }} />
            </label>
          );
        })}
      </div>
    </fieldset>
  );
}

export function StatusPicker({
  value,
  onChange,
  includeArchived = false,
}: {
  value: string;
  onChange: (status: string) => void;
  includeArchived?: boolean;
}) {
  const options = STATUS_ORDER.filter((s) => includeArchived || s !== "archived");
  return (
    <fieldset className="flex flex-col gap-1.5">
      <legend className="label-mono text-text-quiet mb-1.5">Status</legend>
      <div className="border-rule inline-flex w-fit flex-wrap overflow-hidden rounded-md">
        {options.map((s) => (
          <label
            key={s}
            className={`label-mono flex min-h-11 cursor-pointer items-center px-3 ${value === s ? "bg-accent text-on-accent" : "text-text-quiet hover:text-text"}`}
          >
            <input type="radio" name="status" className="sr-only" checked={value === s} onChange={() => onChange(s)} />
            {STATUS_LABEL[s]}
          </label>
        ))}
      </div>
    </fieldset>
  );
}
