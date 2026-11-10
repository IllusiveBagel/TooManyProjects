import { useId, useState } from "react";
import type { Status } from "../api/client";
import { Button, ErrorNote } from "../components/ui";
import { Modal } from "../components/Modal";
import { CATEGORIES, CATEGORY_COLOR, CATEGORY_LABEL } from "./format";
import { useCreateStatus, useDeleteStatus } from "./queries";

/** A text field without a width, for rows that size it themselves. */
export const fieldBase =
  "border-rule bg-surface text-text min-h-10 rounded-md px-3 font-mono text-[13px] outline-none focus:border-accent";
export const fieldClass = `${fieldBase} w-full`;

/** The four categories as buttons, two by two. */
export function CategoryChoice({
  value,
  onChange,
  label = "Counts as",
  big = false,
}: {
  value: string;
  onChange: (c: string) => void;
  label?: string;
  big?: boolean;
}) {
  const id = useId();
  return (
    <div role="radiogroup" aria-labelledby={id} className="flex flex-col gap-1.5">
      <span id={id} className="label-mono text-text-quiet text-[10px]">
        {label}
      </span>
      <div className="grid grid-cols-2 gap-1.5">
        {CATEGORIES.map((c) => (
          <button
            key={c}
            type="button"
            role="radio"
            aria-checked={value === c}
            onClick={() => onChange(c)}
            className={`border-rule flex ${big ? "min-h-12" : "min-h-10"} cursor-pointer items-center gap-2 rounded-md px-2.5 text-left text-[13px] font-semibold whitespace-nowrap ${
              value === c ? "border-accent bg-accent/10 text-text" : "text-text-quiet hover:text-text"
            }`}
          >
            <span className="size-2.5 shrink-0 rounded-full" style={{ background: CATEGORY_COLOR[c] }} aria-hidden="true" />
            {CATEGORY_LABEL[c]}
          </button>
        ))}
      </div>
    </div>
  );
}

/**
 * A new column, made in place. `afterId` is where it goes (null: first);
 * with `choosePlace` the person can change that from a list.
 */
export function NewColumnForm({
  projectId,
  statuses,
  afterId,
  choosePlace = false,
  onDone,
  className = "",
}: {
  projectId: string;
  statuses: Status[];
  afterId: string | null;
  choosePlace?: boolean;
  onDone: () => void;
  className?: string;
}) {
  const create = useCreateStatus(projectId);
  const [name, setName] = useState("");
  const [after, setAfter] = useState<string | null>(afterId);
  const left = after === null ? undefined : statuses.find((s) => s.id === after);
  const [category, setCategory] = useState(left?.category ?? "todo");
  const [touched, setTouched] = useState(false);
  const placeId = useId();
  const pickPlace = (a: string | null) => {
    setAfter(a);
    // Until the person picks one, the category follows the column on its left.
    if (!touched) setCategory((a === null ? undefined : statuses.find((s) => s.id === a))?.category ?? "todo");
  };
  const where = after === null ? "first" : `after ${left?.name ?? "…"}`;

  return (
    <form
      aria-label="New column"
      className={`border-accent bg-surface-raised shadow-card flex flex-col gap-3 rounded-lg border-[1.5px] p-3 ${className}`}
      onSubmit={(e) => {
        e.preventDefault();
        if (!name.trim()) return;
        create.mutate({ name, category, after_id: after }, { onSuccess: onDone });
      }}
      onKeyDown={(e) => e.key === "Escape" && onDone()}
    >
      {!choosePlace && <span className="label-mono text-accent text-[11px]">New column · {where}</span>}
      <label className="flex flex-col gap-1.5">
        <span className="label-mono text-text-quiet text-[10px]">Name</span>
        <input
          autoFocus
          className={fieldClass}
          value={name}
          maxLength={40}
          placeholder="Such as Assembly"
          onChange={(e) => setName(e.target.value)}
        />
      </label>
      <CategoryChoice
        value={category}
        onChange={(c) => {
          setTouched(true);
          setCategory(c);
        }}
      />
      {!touched && left && !choosePlace && (
        <p className="text-text-quiet text-xs leading-snug">
          Picked from the column on its left. This decides where its tasks sit on the global board.
        </p>
      )}
      {choosePlace && (
        <div className="flex flex-col gap-1.5">
          <label htmlFor={placeId} className="label-mono text-text-quiet text-[10px]">
            Goes
          </label>
          <select
            id={placeId}
            className={fieldClass}
            value={after ?? ""}
            onChange={(e) => pickPlace(e.target.value || null)}
          >
            <option value="">At the start</option>
            {statuses.map((s, i) => (
              <option key={s.id} value={s.id}>
                {i === statuses.length - 1 ? `At the end (after ${s.name})` : `After ${s.name}`}
              </option>
            ))}
          </select>
        </div>
      )}
      <ErrorNote error={create.error} />
      <div className="flex gap-2">
        <Button type="button" className="min-h-10 flex-1 px-2 text-sm" onClick={onDone}>
          Cancel
        </Button>
        <Button type="submit" variant="primary" className="min-h-10 flex-1 px-2 text-sm" disabled={!name.trim() || create.isPending}>
          Add column
        </Button>
      </div>
    </form>
  );
}

/** Remove a column, sending its tasks to another one first. */
export function RemoveColumn({
  status,
  statuses,
  count,
  onDone,
  onCancel,
}: {
  status: Status;
  statuses: Status[];
  count: number;
  onDone: () => void;
  onCancel: () => void;
}) {
  const remove = useDeleteStatus(status.project_id);
  const others = statuses.filter((s) => s.id !== status.id);
  const [moveTo, setMoveTo] = useState(others[0]?.id ?? "");
  const lastDone = status.category === "done" && !others.some((s) => s.category === "done");
  const id = useId();
  if (others.length === 0) return <p className="text-text-quiet text-sm">A board needs at least one column.</p>;
  return (
    <div className="flex flex-col gap-2">
      {count > 0 && (
        <div className="flex flex-col gap-1.5">
          <label htmlFor={id} className="label-mono text-text-quiet text-[10px]">
            Move its {count} {count === 1 ? "task" : "tasks"} to
          </label>
          <select id={id} className={fieldClass} value={moveTo} onChange={(e) => setMoveTo(e.target.value)}>
            {others.map((o) => (
              <option key={o.id} value={o.id}>
                {o.name}
              </option>
            ))}
          </select>
        </div>
      )}
      {lastDone && (
        <p className="text-late text-xs leading-snug">
          This is the only column that counts as done. Without it, nothing on this board counts as finished.
        </p>
      )}
      <ErrorNote error={remove.error} />
      <div className="flex gap-2">
        <Button type="button" className="min-h-10 flex-1 px-2 text-sm" onClick={onCancel}>
          Keep it
        </Button>
        <Button
          type="button"
          className="text-late min-h-10 flex-1 border-current px-2 text-sm"
          disabled={remove.isPending}
          onClick={() => remove.mutate({ id: status.id, moveTo: count > 0 ? moveTo : undefined }, { onSuccess: onDone })}
        >
          Remove {status.name}
        </Button>
      </div>
    </div>
  );
}

/** Asks before a column with tasks starts counting as done, since that finishes them all. */
export function ConfirmDone({
  status,
  count,
  onConfirm,
  onCancel,
}: {
  status: Status;
  count: number;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <Modal title={`Make “${status.name}” count as done?`} onClose={onCancel}>
      <p className="mb-4 leading-relaxed">
        {count === 1 ? "Its 1 task" : `Its ${count} tasks`} will count as finished, dated today. Moving the column out of
        Done later makes them open again.
      </p>
      <div className="flex gap-2">
        <Button className="flex-1" onClick={onCancel}>
          Cancel
        </Button>
        <Button variant="primary" className="flex-1" onClick={onConfirm}>
          Count as done
        </Button>
      </div>
    </Modal>
  );
}
