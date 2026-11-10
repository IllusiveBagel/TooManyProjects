import { useState } from "react";
import { Button, ErrorNote } from "../components/ui";
import { Popover } from "./Cell";
import { OPS, opsFor, type Column, type DatabaseDetail, type Filter, type View, type ViewConfig } from "./format";
import { useUpdateView } from "./queries";

const field = "border-rule bg-surface min-h-9 min-w-0 rounded-sm px-2 font-mono text-[12px] outline-none focus:border-accent";
const chip = "label-mono border-rule hover:bg-surface-raised inline-flex min-h-9 cursor-pointer items-center gap-1.5 rounded-md px-2.5 text-[11px]";

/** Filter, sort and column settings for a view, saved as you change them. */
export function ViewControls({ detail, view }: { detail: DatabaseDetail; view: View }) {
  const update = useUpdateView();
  const [open, setOpen] = useState<"filter" | "sort" | "columns" | "layout" | null>(null);
  const save = (config: ViewConfig) => update.mutate({ id: view.id, config });
  const config = view.config;
  const toggle = (k: typeof open) => setOpen((o) => (o === k ? null : k));

  return (
    <div className="contents">
      <div className="relative">
        <button type="button" className={`${chip} ${config.filters.length ? "text-accent" : ""}`} aria-expanded={open === "filter"} onClick={() => toggle("filter")}>
          Filter{config.filters.length ? ` · ${config.filters.length}` : ""}
        </button>
        {open === "filter" && (
          <Popover wide label="Filters" onClose={() => setOpen(null)}>
            <FilterEditor columns={detail.columns} filters={config.filters} onChange={(filters) => save({ ...config, filters })} />
          </Popover>
        )}
      </div>
      <div className="relative">
        <button type="button" className={`${chip} ${config.sorts.length ? "text-accent" : ""}`} aria-expanded={open === "sort"} onClick={() => toggle("sort")}>
          Sort{config.sorts.length ? ` · ${config.sorts.length}` : ""}
        </button>
        {open === "sort" && (
          <Popover wide label="Sorts" onClose={() => setOpen(null)}>
            <SortEditor columns={detail.columns} config={config} onChange={save} />
          </Popover>
        )}
      </div>
      {view.kind === "table" && (
        <div className="relative">
          <button type="button" className={`${chip} ${config.hidden.length ? "text-accent" : ""}`} aria-expanded={open === "columns"} onClick={() => toggle("columns")}>
            Columns{config.hidden.length ? ` · ${config.hidden.length} hidden` : ""}
          </button>
          {open === "columns" && (
            <Popover label="Columns" onClose={() => setOpen(null)}>
              {detail.columns.filter((c) => !c.is_title).map((c) => {
                const shown = !config.hidden.includes(c.id);
                return (
                  <label key={c.id} className="hover:bg-surface-raised flex min-h-10 cursor-pointer items-center gap-2 rounded-sm px-2 text-sm">
                    <input
                      type="checkbox"
                      className="accent-[var(--tmp-accent)]"
                      checked={shown}
                      onChange={() => save({ ...config, hidden: shown ? [...config.hidden, c.id] : config.hidden.filter((h) => h !== c.id) })}
                    />
                    {c.name}
                  </label>
                );
              })}
            </Popover>
          )}
        </div>
      )}
      {(view.kind === "board" || view.kind === "calendar") && (
        <div className="relative">
          <button type="button" className={chip} aria-expanded={open === "layout"} onClick={() => toggle("layout")}>
            {view.kind === "board" ? "Lanes" : "Dates"}
          </button>
          {open === "layout" && (
            <Popover label={view.kind === "board" ? "Lanes" : "Dates"} onClose={() => setOpen(null)}>
              <label className="flex flex-col gap-1 p-1.5">
                <span className="label-mono text-text-quiet text-[10px]">{view.kind === "board" ? "Group by" : "Place rows by"}</span>
                <select
                  className={field}
                  value={(view.kind === "board" ? config.group_by : config.date_column) ?? ""}
                  onChange={(e) =>
                    save(view.kind === "board" ? { ...config, group_by: e.target.value || null } : { ...config, date_column: e.target.value || null })
                  }
                >
                  {detail.columns
                    .filter((c) => c.kind === (view.kind === "board" ? "select" : "date"))
                    .map((c) => (
                      <option key={c.id} value={c.id}>{c.name}</option>
                    ))}
                </select>
              </label>
            </Popover>
          )}
        </div>
      )}
      <ErrorNote error={update.error} />
    </div>
  );
}

function FilterEditor({ columns, filters, onChange }: { columns: Column[]; filters: Filter[]; onChange: (f: Filter[]) => void }) {
  const set = (i: number, f: Partial<Filter>) => onChange(filters.map((x, j) => (j === i ? { ...x, ...f } : x)));
  return (
    <div className="flex flex-col gap-2 p-1.5">
      {filters.length === 0 && <p className="text-text-quiet text-sm">No filters: every row shows.</p>}
      {filters.map((f, i) => {
        const col = columns.find((c) => c.id === f.column_id);
        const ops = opsFor(col?.kind ?? "text");
        const needs = OPS.find((o) => o.op === f.op)?.needsValue ?? false;
        return (
          <div key={i} className="border-rule flex flex-col gap-1.5 rounded-sm p-2">
            <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] gap-1.5">
              <select
                aria-label="Column"
                className={field}
                value={f.column_id}
                onChange={(e) => {
                  const next = columns.find((c) => c.id === e.target.value);
                  set(i, { column_id: e.target.value, op: opsFor(next?.kind ?? "text")[0], value: null });
                }}
              >
                {columns.map((c) => (
                  <option key={c.id} value={c.id}>{c.name}</option>
                ))}
              </select>
              <select aria-label="Condition" className={field} value={f.op} onChange={(e) => set(i, { op: e.target.value })}>
                {OPS.filter((o) => ops.includes(o.op)).map((o) => (
                  <option key={o.op} value={o.op}>{o.label}</option>
                ))}
              </select>
              <button type="button" aria-label="Remove filter" className="text-text-quiet hover:text-late size-9 cursor-pointer" onClick={() => onChange(filters.filter((_, j) => j !== i))}>
                ×
              </button>
            </div>
            {needs && col && <FilterValue column={col} value={f.value} onChange={(value) => set(i, { value })} />}
          </div>
        );
      })}
      <Button
        className="min-h-9 text-sm"
        disabled={columns.length === 0}
        onClick={() => onChange([...filters, { column_id: columns[0]!.id, op: opsFor(columns[0]!.kind)[0]!, value: null }])}
      >
        + Add a filter
      </Button>
    </div>
  );
}

function FilterValue({ column, value, onChange }: { column: Column; value: unknown; onChange: (v: unknown) => void }) {
  const [text, setText] = useState(value === null || value === undefined ? "" : String(value));
  if (column.kind === "select" || column.kind === "multi_select")
    return (
      <select aria-label="Value" className={field} value={String(value ?? "")} onChange={(e) => onChange(e.target.value || null)}>
        <option value="">Choose…</option>
        {column.options.choices.map((c) => (
          <option key={c.id} value={c.id}>{c.name}</option>
        ))}
      </select>
    );
  if (column.kind === "checkbox")
    return (
      <select aria-label="Value" className={field} value={value === true ? "yes" : "no"} onChange={(e) => onChange(e.target.value === "yes")}>
        <option value="yes">Ticked</option>
        <option value="no">Not ticked</option>
      </select>
    );
  const numeric = column.kind === "number" || column.kind === "currency";
  return (
    <input
      aria-label="Value"
      className={field}
      type={column.kind === "date" ? "date" : "text"}
      inputMode={numeric ? "decimal" : undefined}
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={() => onChange(text === "" ? null : numeric && !Number.isNaN(Number(text)) ? Number(text) : text)}
      onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
    />
  );
}

function SortEditor({ columns, config, onChange }: { columns: Column[]; config: ViewConfig; onChange: (c: ViewConfig) => void }) {
  const sorts = config.sorts;
  const set = (next: typeof sorts) => onChange({ ...config, sorts: next });
  return (
    <div className="flex flex-col gap-2 p-1.5">
      {sorts.length === 0 && <p className="text-text-quiet text-sm">No sort: rows keep the order you give them.</p>}
      {sorts.map((s, i) => (
        <div key={i} className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] gap-1.5">
          <select aria-label="Sort by" className={field} value={s.column_id} onChange={(e) => set(sorts.map((x, j) => (j === i ? { ...x, column_id: e.target.value } : x)))}>
            {columns.map((c) => (
              <option key={c.id} value={c.id}>{c.name}</option>
            ))}
          </select>
          <select aria-label="Direction" className={field} value={s.desc ? "desc" : "asc"} onChange={(e) => set(sorts.map((x, j) => (j === i ? { ...x, desc: e.target.value === "desc" } : x)))}>
            <option value="asc">A → Z, low → high</option>
            <option value="desc">Z → A, high → low</option>
          </select>
          <button type="button" aria-label="Remove sort" className="text-text-quiet hover:text-late size-9 cursor-pointer" onClick={() => set(sorts.filter((_, j) => j !== i))}>
            ×
          </button>
        </div>
      ))}
      <Button className="min-h-9 text-sm" onClick={() => set([...sorts, { column_id: columns[0]!.id, desc: false }])}>+ Add a sort</Button>
    </div>
  );
}
