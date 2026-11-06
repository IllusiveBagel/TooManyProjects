import { useQuery } from "@tanstack/react-query";
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { Link } from "react-router";
import { api, unwrap } from "../api/client";
import { initials } from "../sharing/format";
import { useProjectTasks } from "../tasks/queries";
import { projectColor } from "../theme/resolve";
import { cell, display, money, rowTitle, shortDate, type Choice, type Column, type RelationRef, type Row } from "./format";
import { useDatabase, useRows } from "./queries";

export type Names = Record<string, string>;

export function ChoiceTag({ choice }: { choice?: Choice }) {
  if (!choice) return null;
  return (
    <span
      className="label-mono inline-flex max-w-full items-center truncate rounded-sm border px-1.5 py-0.5 text-[10.5px]"
      style={{ color: projectColor(choice.color ?? "slot-7"), borderColor: "currentColor" }}
    >
      {choice.name}
    </span>
  );
}

/** Where a relation link goes. */
export function relationHref(r: RelationRef): string {
  switch (r.kind) {
    case "task":
      return `/projects/${r.project_id}/board?task=${r.id}`;
    case "note":
      return `/notes/${r.id}`;
    case "project":
      return `/projects/${r.id}`;
    default:
      return `/projects/${r.project_id}/data/${r.database_id}`;
  }
}

/** A cell's value, read only. */
export function CellValue({ column, row, names }: { column: Column; row: Row; names: Names }) {
  const v = cell(row, column.id);
  switch (column.kind) {
    case "select":
      return <ChoiceTag choice={column.options.choices.find((c) => c.id === v)} />;
    case "multi_select":
      return (
        <span className="flex flex-wrap gap-1">
          {((v as string[] | undefined) ?? []).map((id) => (
            <ChoiceTag key={id} choice={column.options.choices.find((c) => c.id === id)} />
          ))}
        </span>
      );
    case "checkbox":
      return <span aria-label={v ? "Yes" : "No"} className={v ? "text-accent" : "text-text-quiet"}>{v ? "☑" : "☐"}</span>;
    case "url":
      return typeof v === "string" ? (
        <a href={v} target="_blank" rel="noreferrer noopener" className="text-accent truncate underline decoration-1 underline-offset-4" onClick={(e) => e.stopPropagation()}>
          {v.replace(/^https?:\/\/(www\.)?/, "")}
        </a>
      ) : null;
    case "number":
    case "currency":
      return v === undefined ? null : <span className="font-mono text-[13px] tabular-nums">{display(column, row)}</span>;
    case "date":
      return typeof v === "string" ? <span className="font-mono text-[12.5px]">{shortDate(v)}</span> : null;
    case "person":
      return typeof v === "string" ? (
        <span className="flex items-center gap-1.5">
          <span className="bg-surface-raised label-mono grid size-6 shrink-0 place-items-center rounded-full text-[9px] font-semibold">
            {initials(names[v] ?? "?")}
          </span>
          <span className="truncate">{names[v] ?? "Someone"}</span>
        </span>
      ) : null;
    case "relation":
      return (
        <span className="flex flex-wrap gap-x-2 gap-y-0.5">
          {(row.relations[column.id] ?? []).map((r) => (
            <Link key={r.id} to={relationHref(r)} onClick={(e) => e.stopPropagation()} className="text-accent truncate border-b border-current/40">
              {r.title}
            </Link>
          ))}
        </span>
      );
    default:
      return typeof v === "string" ? <span className="line-clamp-2 break-words">{v}</span> : null;
  }
}

const inputClass = "border-accent bg-surface w-full min-w-0 rounded-sm border-2 px-2 py-1.5 outline-none";

export interface CellChange {
  values?: Record<string, unknown>;
  relations?: Record<string, { kind: string; id: string }[]>;
}

/**
 * An editor for one cell. Text-like kinds save on Enter or when focus
 * leaves; pickers save on each pick. Escape closes without saving.
 */
export function CellEditor({
  column,
  row,
  projectId,
  members,
  onSave,
  onClose,
}: {
  column: Column;
  row: Row;
  projectId: string;
  members: { user_id: string; name: string }[];
  onSave: (change: CellChange) => void;
  onClose: () => void;
}) {
  const v = cell(row, column.id);
  const set = (value: unknown) => onSave({ values: { [column.id]: value } });
  const keys = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  };

  switch (column.kind) {
    case "select":
      return (
        <select
          autoFocus
          aria-label={column.name}
          className={`${inputClass} font-mono text-[13px]`}
          value={typeof v === "string" ? v : ""}
          onKeyDown={keys}
          onBlur={onClose}
          onChange={(e) => {
            set(e.target.value || null);
            onClose();
          }}
        >
          <option value="">—</option>
          {column.options.choices.map((c) => (
            <option key={c.id} value={c.id}>{c.name}</option>
          ))}
        </select>
      );
    case "person":
      return (
        <select
          autoFocus
          aria-label={column.name}
          className={`${inputClass} font-mono text-[13px]`}
          value={typeof v === "string" ? v : ""}
          onKeyDown={keys}
          onBlur={onClose}
          onChange={(e) => {
            set(e.target.value || null);
            onClose();
          }}
        >
          <option value="">Nobody</option>
          {members.map((m) => (
            <option key={m.user_id} value={m.user_id}>{m.name}</option>
          ))}
        </select>
      );
    case "date":
      return (
        <input
          autoFocus
          type="date"
          aria-label={column.name}
          className={`${inputClass} font-mono text-[13px]`}
          defaultValue={typeof v === "string" ? v : ""}
          onKeyDown={keys}
          onBlur={(e) => {
            if (e.target.value !== (v ?? "")) set(e.target.value || null);
            onClose();
          }}
        />
      );
    case "multi_select":
      return (
        <Popover onClose={onClose} label={column.name}>
          {column.options.choices.length === 0 && <p className="text-text-quiet p-2 text-sm">No choices yet. Add some from the column's menu.</p>}
          {column.options.choices.map((c) => {
            const list = (v as string[] | undefined) ?? [];
            const on = list.includes(c.id);
            return (
              <label key={c.id} className="hover:bg-surface-raised flex min-h-10 cursor-pointer items-center gap-2 rounded-sm px-2">
                <input
                  type="checkbox"
                  className="accent-[var(--tmp-accent)]"
                  checked={on}
                  onChange={() => set(on ? list.filter((x) => x !== c.id) : [...list, c.id])}
                />
                <ChoiceTag choice={c} />
              </label>
            );
          })}
        </Popover>
      );
    case "relation":
      return <RelationPicker column={column} row={row} projectId={projectId} onSave={onSave} onClose={onClose} />;
    default: {
      const multiline = column.kind === "text" && !column.is_title;
      const initial = column.kind === "number" || column.kind === "currency" ? (v === undefined ? "" : String(v)) : ((v as string) ?? "");
      return (
        <TextEditor
          multiline={multiline}
          initial={initial}
          label={column.name}
          numeric={column.kind === "number" || column.kind === "currency"}
          onSave={(text) => {
            if (text !== initial) set(text === "" ? null : text);
            onClose();
          }}
          onCancel={onClose}
        />
      );
    }
  }
}

function TextEditor({
  multiline,
  initial,
  label,
  numeric,
  onSave,
  onCancel,
}: {
  multiline: boolean;
  initial: string;
  label: string;
  numeric: boolean;
  onSave: (text: string) => void;
  onCancel: () => void;
}) {
  const [text, setText] = useState(initial);
  const done = useRef(false);
  const finish = (save: boolean) => {
    if (done.current) return;
    done.current = true;
    if (save) onSave(text.trim() === "" ? "" : text);
    else onCancel();
  };
  const keys = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      finish(false);
    } else if (e.key === "Enter" && !(multiline && e.shiftKey)) {
      e.preventDefault();
      finish(true);
    }
  };
  const props = {
    autoFocus: true,
    "aria-label": label,
    value: text,
    onKeyDown: keys,
    onBlur: () => finish(true),
    onFocus: (e: React.FocusEvent<HTMLInputElement | HTMLTextAreaElement>) => e.target.select(),
  };
  return multiline ? (
    <textarea {...props} rows={3} className={`${inputClass} resize-y text-sm`} onChange={(e) => setText(e.target.value)} />
  ) : (
    <input
      {...props}
      inputMode={numeric ? "decimal" : undefined}
      className={`${inputClass} ${numeric ? "text-right font-mono text-[13px]" : "text-sm"}`}
      onChange={(e) => setText(e.target.value)}
    />
  );
}

/** A small panel under the cell that closes on Escape or a click elsewhere. */
export function Popover({
  children,
  onClose,
  label,
  align = "left",
  wide = false,
}: {
  children: ReactNode;
  onClose: () => void;
  label: string;
  align?: "left" | "right";
  wide?: boolean;
}) {
  // Drawn on the page body so a scrolling table or drawer can't clip it,
  // placed under the element that holds it (or above, when there's no room).
  const anchor = useRef<HTMLSpanElement>(null);
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const place = () => {
      const host = anchor.current?.parentElement;
      const el = ref.current;
      if (!host || !el) return;
      const r = host.getBoundingClientRect();
      const vw = window.innerWidth;
      const vh = window.innerHeight;
      const w = el.offsetWidth;
      const left = align === "right" ? r.right - w : r.left;
      el.style.left = `${Math.max(8, Math.min(left, vw - w - 8))}px`;
      const below = vh - r.bottom - 12;
      const above = r.top - 12;
      if (below >= 240 || below >= above) {
        el.style.top = `${r.bottom + 4}px`;
        el.style.bottom = "";
        el.style.maxHeight = `${Math.min(320, below)}px`;
      } else {
        el.style.top = "";
        el.style.bottom = `${vh - r.top + 4}px`;
        el.style.maxHeight = `${Math.min(320, above)}px`;
      }
      el.style.visibility = "visible";
    };
    place();
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    const observer = new ResizeObserver(place);
    if (ref.current) observer.observe(ref.current);
    return () => {
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
      observer.disconnect();
    };
  }, [align]);
  useEffect(() => {
    const down = (e: PointerEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const key = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    document.addEventListener("pointerdown", down);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("pointerdown", down);
      document.removeEventListener("keydown", key);
    };
  }, [onClose]);
  return (
    <>
      <span ref={anchor} hidden />
      {createPortal(
        <div
          ref={ref}
          role="dialog"
          aria-label={label}
          style={{ visibility: "hidden" }}
          className={`border-rule bg-surface-card shadow-card fixed z-[60] flex ${wide ? "w-[26rem]" : "w-72"} max-w-[calc(100vw-16px)] flex-col gap-0.5 overflow-y-auto rounded-md p-1.5`}
        >
          {children}
        </div>,
        document.body,
      )}
    </>
  );
}

interface Option {
  id: string;
  title: string;
  hint?: string;
}

/** Things a relation column can point at, with a search. */
function useTargets(column: Column, projectId: string, q: string): Option[] {
  const target = column.options.target;
  const tasks = useProjectTasks(target === "task" ? projectId : "");
  const notes = useQuery({
    queryKey: ["notes", "list", { q: q || undefined, limit: 30 }],
    queryFn: () => unwrap(api.GET("/notes", { params: { query: { q: q || undefined, limit: 30 } } })),
    enabled: target === "note",
    placeholderData: (prev) => prev,
  });
  const projects = useQuery({
    queryKey: ["projects", "list"],
    queryFn: () => unwrap(api.GET("/projects")),
    enabled: target === "project",
  });
  const other = useDatabase(target === "row" ? (column.options.target_database_id ?? undefined) : undefined);
  const rows = useRows(target === "row" ? (column.options.target_database_id ?? undefined) : undefined, null, q);
  const needle = q.trim().toLowerCase();
  const match = (s: string) => !needle || s.toLowerCase().includes(needle);
  return useMemo(() => {
    switch (target) {
      case "task":
        return (tasks.data ?? [])
          .map((t) => ({ id: t.id, title: t.title, hint: `T-${t.number}` }))
          .filter((o) => match(`${o.hint} ${o.title}`));
      case "note":
        return (notes.data ?? []).map((n) => ({ id: n.id, title: n.title, hint: n.folder }));
      case "project":
        return (projects.data ?? []).map((p) => ({ id: p.id, title: p.title })).filter((o) => match(o.title));
      case "row":
        return (rows.data ?? []).map((r) => ({ id: r.id, title: rowTitle(other.data?.columns ?? [], r) }));
      default:
        return [];
    }
  }, [target, tasks.data, notes.data, projects.data, rows.data, other.data, needle]);
}

function RelationPicker({
  column,
  row,
  projectId,
  onSave,
  onClose,
}: {
  column: Column;
  row: Row;
  projectId: string;
  onSave: (change: CellChange) => void;
  onClose: () => void;
}) {
  const [q, setQ] = useState("");
  const current = row.relations[column.id] ?? [];
  const options = useTargets(column, projectId, q);
  const kind = column.options.target ?? "task";
  const toggle = (id: string) => {
    const ids = current.map((r) => r.id);
    const next = ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id];
    onSave({ relations: { [column.id]: next.map((x) => ({ kind, id: x })) } });
  };
  return (
    <Popover onClose={onClose} label={`Link ${column.name}`}>
      <input
        autoFocus
        className="border-rule bg-surface mb-1 min-h-10 rounded-sm px-2 font-mono text-[13px] outline-none focus:border-accent"
        placeholder="search"
        value={q}
        onChange={(e) => setQ(e.target.value)}
        aria-label="Search"
      />
      {options.length === 0 && <p className="text-text-quiet p-2 text-sm">Nothing to link.</p>}
      {options.slice(0, 60).map((o) => {
        const on = current.some((r) => r.id === o.id);
        return (
          <label key={o.id} className="hover:bg-surface-raised flex min-h-10 cursor-pointer items-center gap-2 rounded-sm px-2 text-sm">
            <input type="checkbox" className="accent-[var(--tmp-accent)]" checked={on} onChange={() => toggle(o.id)} />
            {o.hint && <span className="label-mono text-text-quiet text-[10px]">{o.hint}</span>}
            <span className="truncate">{o.title}</span>
          </label>
        );
      })}
    </Popover>
  );
}

export { money };
