import { useState } from "react";
import { Button, ErrorNote } from "../components/ui";
import { ChoiceTag, Popover } from "./Cell";
import { KIND_LABEL, KINDS, SLOTS, TARGET_LABEL, type Choice, type Column, type ColumnOptions, type DatabaseDetail, type Kind, type View } from "./format";
import { useAllDatabases, useCreateColumn, useDeleteColumn, useUpdateColumn, useUpdateView } from "./queries";
import { HeaderLabel } from "./TableView";

const field = "border-rule bg-surface min-h-10 w-full rounded-sm px-2 font-mono text-[13px] outline-none focus:border-accent";

/**
 * A column header that opens its settings: name, type, choices, hide, move,
 * delete. Without a column it is the "+" that adds one.
 */
export function ColumnMenu({ column, detail, view }: { column?: Column; detail: DatabaseDetail; view: View | null }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="relative">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        aria-label={column ? `${column.name} column settings` : "Add a column"}
        className="hover:bg-surface-raised w-full cursor-pointer rounded-sm text-left"
      >
        {column ? <HeaderLabel column={column} /> : <span className="label-mono text-text-quiet grid min-h-10 place-items-center text-sm">+</span>}
      </button>
      {open && (
        <Popover onClose={() => setOpen(false)} label={column ? `${column.name} settings` : "New column"}>
          <ColumnForm column={column} detail={detail} view={view} onDone={() => setOpen(false)} />
        </Popover>
      )}
    </div>
  );
}

function ColumnForm({ column, detail, view, onDone }: { column?: Column; detail: DatabaseDetail; view: View | null; onDone: () => void }) {
  const create = useCreateColumn(detail.database.id);
  const update = useUpdateColumn();
  const remove = useDeleteColumn();
  const updateView = useUpdateView();
  const [name, setName] = useState(column?.name ?? "");
  const [kind, setKind] = useState<Kind>((column?.kind as Kind) ?? "text");
  const [options, setOptions] = useState<ColumnOptions>(column?.options ?? { choices: [] });
  const [confirm, setConfirm] = useState(false);
  const others = useAllDatabases(kind === "relation" && options.target === "row").data ?? [];

  const changedKind = column && kind !== column.kind;
  const submit = () => {
    const body = { name: name.trim() || KIND_LABEL[kind], kind, options };
    if (!column) create.mutate(body, { onSuccess: onDone });
    else update.mutate({ id: column.id, ...body, options: changedKind && options === column.options ? undefined : options }, { onSuccess: onDone });
  };
  const columns = detail.columns;
  const index = column ? columns.findIndex((c) => c.id === column.id) : -1;
  const move = (dir: -1 | 1) => {
    if (!column) return;
    // after_id is the column it should follow; moving left means following the one two to the left.
    const target = dir === -1 ? (columns[index - 2]?.id ?? null) : (columns[index + 1]?.id ?? null);
    if (dir === 1 && !columns[index + 1]) return;
    update.mutate({ id: column.id, after_id: target });
  };

  return (
    <form
      className="flex flex-col gap-2.5 p-1.5"
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <input autoFocus className={field} value={name} maxLength={60} placeholder="Column name" aria-label="Column name" onChange={(e) => setName(e.target.value)} />
      {!column?.is_title && (
        <label className="flex flex-col gap-1">
          <span className="label-mono text-text-quiet text-[10px]">Type</span>
          <select className={field} value={kind} onChange={(e) => setKind(e.target.value as Kind)}>
            {KINDS.map((k) => (
              <option key={k} value={k}>{KIND_LABEL[k]}</option>
            ))}
          </select>
        </label>
      )}
      {changedKind && <p className="text-text-quiet text-xs">Values that fit the new type are kept; the rest are cleared.</p>}
      {(kind === "select" || kind === "multi_select") && (
        <ChoicesEditor
          choices={options.choices}
          onChange={(choices) => setOptions({ ...options, choices })}
          note={changedKind && column?.kind === "text" ? "Leave empty to make a choice of each value already in the column." : undefined}
        />
      )}
      {kind === "currency" && (
        <label className="flex flex-col gap-1">
          <span className="label-mono text-text-quiet text-[10px]">Currency</span>
          <select className={field} value={options.currency ?? "GBP"} onChange={(e) => setOptions({ ...options, currency: e.target.value })}>
            {["GBP", "EUR", "USD", "AUD", "CAD", "JPY"].map((c) => (
              <option key={c}>{c}</option>
            ))}
          </select>
        </label>
      )}
      {kind === "relation" && (
        <>
          <label className="flex flex-col gap-1">
            <span className="label-mono text-text-quiet text-[10px]">Links to</span>
            <select className={field} value={options.target ?? ""} onChange={(e) => setOptions({ ...options, target: e.target.value || null })}>
              <option value="">Choose…</option>
              {Object.entries(TARGET_LABEL).map(([k, label]) => (
                <option key={k} value={k}>{label}</option>
              ))}
            </select>
          </label>
          {options.target === "row" && (
            <select
              aria-label="Database"
              className={field}
              value={options.target_database_id ?? ""}
              onChange={(e) => setOptions({ ...options, target_database_id: e.target.value || null })}
            >
              <option value="">Which database?</option>
              {others.map((d) => (
                <option key={d.id} value={d.id}>{d.name} · {d.project_title}</option>
              ))}
            </select>
          )}
        </>
      )}
      <ErrorNote error={create.error ?? update.error ?? remove.error ?? updateView.error} />
      <Button type="submit" variant="primary" className="min-h-9 text-sm">{column ? "Save" : "Add column"}</Button>
      {column && (
        <div className="border-rule flex flex-wrap gap-1 border-x-0 border-b-0 pt-2">
          <Button type="button" className="min-h-9 px-2 text-xs" disabled={index <= 0} onClick={() => move(-1)} aria-label="Move left">←</Button>
          <Button type="button" className="min-h-9 px-2 text-xs" disabled={index >= columns.length - 1} onClick={() => move(1)} aria-label="Move right">→</Button>
          {view && !column.is_title && (
            <Button
              type="button"
              className="min-h-9 px-2 text-xs"
              onClick={() =>
                updateView.mutate(
                  { id: view.id, config: { ...view.config, hidden: [...view.config.hidden, column.id] } },
                  { onSuccess: onDone },
                )
              }
            >
              Hide in {view.name}
            </Button>
          )}
          {!column.is_title &&
            (confirm ? (
              <Button type="button" className="text-late min-h-9 px-2 text-xs" onClick={() => remove.mutate(column.id, { onSuccess: onDone })}>
                Delete column and its values
              </Button>
            ) : (
              <Button type="button" className="text-late min-h-9 px-2 text-xs" onClick={() => setConfirm(true)}>Delete…</Button>
            ))}
        </div>
      )}
    </form>
  );
}

export function ChoicesEditor({ choices, onChange, note }: { choices: Choice[]; onChange: (c: Choice[]) => void; note?: string }) {
  const [adding, setAdding] = useState("");
  const add = () => {
    const n = adding.trim();
    if (!n || choices.some((c) => c.name.toLowerCase() === n.toLowerCase())) return;
    onChange([...choices, { id: "", name: n, color: SLOTS[choices.length % SLOTS.length] }]);
    setAdding("");
  };
  return (
    <div className="flex flex-col gap-1">
      <span className="label-mono text-text-quiet text-[10px]">Choices</span>
      {note && choices.length === 0 && <p className="text-text-quiet text-xs">{note}</p>}
      <ul className="flex flex-col gap-1">
        {choices.map((c, i) => (
          <li key={c.id || c.name} className="flex items-center gap-1.5">
            <button
              type="button"
              title="Change colour"
              aria-label={`Change colour of ${c.name}`}
              className="cursor-pointer"
              onClick={() => {
                const next = SLOTS[(SLOTS.indexOf(c.color ?? "slot-1") + 1) % SLOTS.length];
                onChange(choices.map((x, j) => (j === i ? { ...x, color: next } : x)));
              }}
            >
              <ChoiceTag choice={c} />
            </button>
            <button
              type="button"
              aria-label={`Remove ${c.name}`}
              className="text-text-quiet hover:text-late ml-auto size-8 cursor-pointer"
              onClick={() => onChange(choices.filter((_, j) => j !== i))}
            >
              ×
            </button>
          </li>
        ))}
      </ul>
      <input
        className={field}
        placeholder="+ Add a choice"
        aria-label="New choice"
        value={adding}
        maxLength={60}
        onChange={(e) => setAdding(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            add();
          }
        }}
        onBlur={add}
      />
    </div>
  );
}
