import { useState } from "react";
import { Button } from "../components/ui";
import { cell, monthGrid, rowTitle, type Row } from "./format";
import { useCreateRow } from "./queries";
import type { ViewProps } from "./TableView";

/** Rows on a month grid by the view's date column. Phones get a list. */
export function CalendarView({ detail, view, rows, onOpenRow }: ViewProps) {
  const dateColumn = detail.columns.find((c) => c.id === view?.config.date_column && c.kind === "date");
  const create = useCreateRow(detail.database.id);
  const titleColumn = detail.columns.find((c) => c.is_title)!;
  const today = new Date();
  const [month, setMonth] = useState({ y: today.getFullYear(), m: today.getMonth() });
  if (!dateColumn)
    return <p className="text-text-quiet p-4 text-sm">A calendar needs a date column. Add one, then pick it in this view's settings.</p>;

  const byDay = new Map<string, Row[]>();
  const undated: Row[] = [];
  for (const r of rows) {
    const d = cell(r, dateColumn.id);
    if (typeof d === "string") byDay.set(d, [...(byDay.get(d) ?? []), r]);
    else undated.push(r);
  }
  const days = monthGrid(month.y, month.m);
  const todayIso = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, "0")}-${String(today.getDate()).padStart(2, "0")}`;
  const label = new Date(month.y, month.m, 1).toLocaleDateString("en-GB", { month: "long", year: "numeric" });
  const step = (d: number) => setMonth(({ y, m }) => ({ y: m + d < 0 ? y - 1 : m + d > 11 ? y + 1 : y, m: (m + d + 12) % 12 }));
  const add = (iso: string) => {
    if (!detail.can_edit) return;
    const name = prompt(`New row on ${iso}`);
    if (name?.trim()) create.mutate({ values: { [titleColumn.id]: name.trim(), [dateColumn.id]: iso }, relations: {} });
  };
  const inMonth = days.filter((d) => d.inMonth && byDay.has(d.iso));

  return (
    <div className="flex flex-col gap-3 p-3 sm:p-4">
      <div className="flex items-center gap-2">
        <h3 className="font-display flex-1 text-lg font-extrabold">{label}</h3>
        <Button className="min-h-9 px-3 text-sm" aria-label="Previous month" onClick={() => step(-1)}>←</Button>
        <Button className="min-h-9 px-3 text-sm" onClick={() => setMonth({ y: today.getFullYear(), m: today.getMonth() })}>Today</Button>
        <Button className="min-h-9 px-3 text-sm" aria-label="Next month" onClick={() => step(1)}>→</Button>
      </div>

      <div className="hidden grid-cols-7 sm:grid">
        {["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].map((d) => (
          <span key={d} className="label-mono text-text-quiet px-1.5 pb-1 text-[10px]">{d}</span>
        ))}
        {days.map((d) => (
          <div
            key={d.iso}
            className={`border-rule flex min-h-24 flex-col gap-1 border-t-0 border-l-0 p-1.5 ${d.inMonth ? "" : "opacity-45"}`}
            onDoubleClick={() => add(d.iso)}
          >
            <span className={`label-mono self-start rounded-sm px-1 text-[11px] ${d.iso === todayIso ? "bg-accent text-on-accent" : "text-text-quiet"}`}>
              {Number(d.iso.slice(8))}
            </span>
            {(byDay.get(d.iso) ?? []).slice(0, 4).map((r) => (
              <button
                key={r.id}
                type="button"
                onClick={() => onOpenRow(r)}
                className="bg-surface-raised hover:text-accent cursor-pointer truncate rounded-sm px-1.5 py-1 text-left text-xs font-semibold"
              >
                {rowTitle(detail.columns, r)}
              </button>
            ))}
            {(byDay.get(d.iso)?.length ?? 0) > 4 && (
              <span className="label-mono text-text-quiet text-[10px]">+{byDay.get(d.iso)!.length - 4} more</span>
            )}
          </div>
        ))}
      </div>

      <ol className="flex flex-col sm:hidden">
        {inMonth.length === 0 && <li className="text-text-quiet py-2 text-sm">Nothing this month.</li>}
        {inMonth.map((d) => (
          <li key={d.iso} className="border-rule flex gap-3 border-x-0 border-t-0 py-2">
            <span className={`label-mono w-12 shrink-0 text-[11px] ${d.iso === todayIso ? "text-accent" : "text-text-quiet"}`}>
              {new Date(`${d.iso}T00:00:00`).toLocaleDateString("en-GB", { weekday: "short", day: "numeric" })}
            </span>
            <span className="flex flex-1 flex-col gap-1">
              {byDay.get(d.iso)!.map((r) => (
                <button key={r.id} type="button" className="cursor-pointer text-left font-semibold" onClick={() => onOpenRow(r)}>
                  {rowTitle(detail.columns, r)}
                </button>
              ))}
            </span>
          </li>
        ))}
      </ol>

      {undated.length > 0 && (
        <p className="label-mono text-text-quiet text-[11px]">
          {undated.length} row{undated.length === 1 ? "" : "s"} without a {dateColumn.name.toLowerCase()}
        </p>
      )}
    </div>
  );
}
