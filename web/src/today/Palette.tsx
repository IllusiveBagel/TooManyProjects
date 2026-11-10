import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useLocation, useNavigate } from "react-router";
import { create } from "zustand";
import { useUpdatePreferences } from "../auth/queries";
import { useCanvas } from "../canvas/queries";
import { useCanvasUi } from "../canvas/store";
import { useQuickNote } from "../notes/QuickNote";
import { useCreateNote } from "../notes/queries";
import {
  FILTERS,
  filterHits,
  groupHits,
  hitPath,
  lastProject,
  matches,
  pushRecent,
  readRecent,
  snippetParts,
  type FilterId,
  type SearchHit,
} from "./format";
import { useQuickAddUi } from "./QuickAdd";
import { useQuickAdd, useSearch } from "./queries";

interface PaletteState {
  open: boolean;
  show: () => void;
  hide: () => void;
}

export const usePalette = create<PaletteState>((set) => ({
  open: false,
  show: () => set({ open: true }),
  hide: () => set({ open: false }),
}));

/** Ctrl+K (Cmd+K on a Mac) anywhere opens the palette, even from inside an editor. */
export function usePaletteShortcut() {
  const { show, hide } = usePalette();
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        e.stopPropagation();
        if (usePalette.getState().open) hide();
        else show();
      }
    };
    window.addEventListener("keydown", onKey, { capture: true });
    return () => window.removeEventListener("keydown", onKey, { capture: true });
  }, [show, hide]);
}

interface Item {
  key: string;
  kind: string;
  /** Results are coloured; actions aren't. */
  isHit: boolean;
  title: ReactNode;
  meta?: string;
  hint?: string;
  run: () => void;
}

const SEARCH_ICON = "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zM20 20l-4-4";

function useDebounced(value: string, ms: number) {
  const [v, setV] = useState(value);
  useEffect(() => {
    const t = window.setTimeout(() => setV(value), ms);
    return () => window.clearTimeout(t);
  }, [value, ms]);
  return v;
}

function hitTitle(h: SearchHit): ReactNode {
  if (h.kind === "comment" && h.snippet) {
    return snippetParts(h.snippet).map((p, i) => (p.hit ? <mark key={i} className="text-text bg-transparent underline decoration-2 underline-offset-2">{p.text}</mark> : <span key={i}>{p.text}</span>));
  }
  return h.title;
}

function hitMeta(h: SearchHit): string {
  const day = new Date(h.updated_at).toLocaleDateString("en-GB", { day: "numeric", month: "short" });
  switch (h.kind) {
    case "task":
      return [h.project_title, h.done ? "Done" : h.context].filter(Boolean).join(" · ");
    case "comment":
      return `On T-${h.task_number} · ${day}`;
    case "row":
      return [h.project_title, h.context].filter(Boolean).join(" · ");
    case "note":
      return h.context ?? "";
    case "project":
      return h.context ?? "";
    default:
      return "";
  }
}

export function Palette() {
  const open = usePalette((s) => s.open);
  if (!open) return null;
  return <PaletteBody />;
}

function PaletteBody() {
  const hide = usePalette((s) => s.hide);
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const [q, setQ] = useState("");
  const [filter, setFilter] = useState<FilterId>("all");
  const [active, setActive] = useState(0);
  const query = useDebounced(q, 120);
  const search = useSearch(query);
  const projects = useCanvas(false).data?.projects ?? [];
  const startAdd = useQuickAddUi((s) => s.start);
  const doneAdd = useQuickAddUi((s) => s.done);
  const startNote = useQuickNote((s) => s.start);
  const startNewProject = useCanvasUi((s) => s.startNewProject);
  const prefs = useUpdatePreferences();
  const add = useQuickAdd();
  const createNote = useCreateNote();
  const list = useRef<HTMLDivElement>(null);

  const go = (path: string) => {
    hide();
    navigate(path);
  };
  const openHit = (h: SearchHit) => {
    pushRecent(h);
    go(hitPath(h));
  };

  // The project "New task in …" goes to: the one on screen, else the last used.
  const here = /^\/projects\/([^/]+)/.exec(pathname)?.[1];
  const target = projects.find((p) => p.id === (here ?? lastProject()) && p.role !== "viewer");
  const text = q.trim();

  const newTask = () => {
    if (!target || !text) return;
    add.mutate(
      { text, kind: "task", project_id: target.id },
      {
        onSuccess: (r) => {
          hide();
          doneAdd({ text: `Added to ${target.title} · ${r.column}`, path: `/projects/${target.id}/board?task=${r.task?.id}` });
        },
      },
    );
  };

  const groups = useMemo(() => {
    const out: { title: string; items: Item[] }[] = [];
    const toItem = (h: SearchHit): Item => ({
      key: `${h.kind}:${h.id}`,
      kind: h.kind,
      isHit: true,
      title: hitTitle(h),
      meta: hitMeta(h),
      run: () => openHit(h),
    });
    if (!text) {
      const recent = filterHits(readRecent(), filter);
      if (recent.length) out.push({ title: "Recent", items: recent.map(toItem) });
    } else if (search.data && query === text) {
      for (const g of groupHits(filterHits(search.data, filter))) out.push({ title: g.title, items: g.hits.map(toItem) });
    }
    if (filter !== "all") return out;

    const actions: Item[] = [];
    if (text && target) {
      actions.push({ key: "new-task", kind: "action", isHit: false, title: `New task in ${target.title}: “${text}”`, hint: "Alt Enter", run: newTask });
    }
    if (text) {
      actions.push({ key: "quick-add", kind: "action", isHit: false, title: `Quick add “${text}”`, hint: "Alt A", run: () => (hide(), startAdd({ text })) });
    }
    const firstProject = text ? search.data?.find((h) => h.kind === "project") : undefined;
    if (firstProject) {
      for (const [tab, label] of [["board", "board"], ["notes", "notes"], ["data", "data"], ["photos", "photos"]] as const) {
        actions.push({ key: `p-${tab}`, kind: "go to", isHit: false, title: `${firstProject.title} ${label}`, run: () => go(`/projects/${firstProject.id}/${tab}`) });
      }
    }
    const fixed: Item[] = [
      { key: "a-add", kind: "action", isHit: false, title: "Quick add", hint: "Alt A", run: () => (hide(), startAdd()) },
      { key: "a-note", kind: "action", isHit: false, title: text ? `New note “${text}”` : "New note", hint: "Alt N", run: () => (hide(), startNote()) },
      { key: "a-plan", kind: "action", isHit: false, title: "Plan time", run: () => go("/calendar?plan=new") },
      { key: "a-project", kind: "action", isHit: false, title: "New project", run: () => (go("/"), startNewProject()) },
      { key: "g-today", kind: "go to", isHit: false, title: "Today", run: () => go("/today") },
      { key: "g-canvas", kind: "go to", isHit: false, title: "Canvas", run: () => go("/") },
      { key: "g-board", kind: "go to", isHit: false, title: "Board", run: () => go("/board") },
      { key: "g-calendar", kind: "go to", isHit: false, title: "Calendar", run: () => go("/calendar") },
      { key: "g-notes", kind: "go to", isHit: false, title: "Notes", run: () => go("/notes") },
      { key: "g-settings", kind: "go to", isHit: false, title: "Settings", run: () => go("/settings") },
      { key: "t-dark", kind: "theme", isHit: false, title: "Switch to dark", run: () => (hide(), prefs.mutate({ theme_mode: "dark" })) },
      { key: "t-light", kind: "theme", isHit: false, title: "Switch to light", run: () => (hide(), prefs.mutate({ theme_mode: "light" })) },
      { key: "t-system", kind: "theme", isHit: false, title: "Match the system theme", run: () => (hide(), prefs.mutate({ theme_mode: "system" })) },
    ];
    // With words typed, only the fixed actions they name (New note is the exception: it takes the words).
    for (const f of fixed) {
      if (!text) actions.push(f);
      else if (f.key === "a-note" || matches(text, `${f.kind} ${String(f.title)}`)) {
        if (f.key === "a-add") continue;
        actions.push(
          f.key === "a-note"
            ? { ...f, run: () => createNote.mutate({ title: text, project_ids: target ? [target.id] : [] }, { onSuccess: (n) => go(`/notes/${n.note.id}`) }) }
            : f,
        );
      }
    }
    if (actions.length) out.push({ title: "Actions", items: actions });
    return out;
  }, [text, query, search.data, filter, target?.id]);

  const flat = groups.flatMap((g) => g.items);
  useEffect(() => setActive(0), [query, filter]);
  useEffect(() => {
    list.current?.querySelector(`[data-index="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }, [active]);

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((a) => Math.min(a + 1, flat.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => Math.max(a - 1, 0));
    } else if (e.key === "Enter" && e.altKey) {
      e.preventDefault();
      newTask();
    } else if (e.key === "Enter") {
      e.preventDefault();
      flat[active]?.run();
    } else if (e.key === "Tab") {
      e.preventDefault();
      const i = FILTERS.findIndex((f) => f.id === filter);
      const n = FILTERS.length;
      setFilter(FILTERS[(i + (e.shiftKey ? n - 1 : 1)) % n]!.id);
    } else if (e.key === "Escape") {
      e.preventDefault();
      hide();
    } else if (e.altKey && e.code === "KeyA") {
      e.preventDefault();
      e.stopPropagation();
      hide();
      startAdd({ text });
    }
  };

  let index = -1;
  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center sm:px-4 sm:pt-[12vh]">
      <div className="bg-shadow/60 absolute inset-0" onClick={hide} aria-hidden="true" />
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Search"
        className="border-rule bg-surface-card shadow-card relative flex h-dvh w-full max-w-[680px] flex-col overflow-hidden sm:h-auto sm:max-h-[70vh] sm:rounded-lg"
      >
        <div className="border-rule flex items-center gap-3 border-x-0 border-t-0 px-4 py-3">
          <svg viewBox="0 0 24 24" className="text-text-quiet size-5 shrink-0" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
            <path d={SEARCH_ICON} />
          </svg>
          <input
            autoFocus
            role="combobox"
            aria-expanded="true"
            aria-controls="palette-list"
            aria-activedescendant={flat[active] ? `palette-${active}` : undefined}
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={onKey}
            placeholder="Search tasks, notes, data and projects, or type a command"
            className="min-h-11 min-w-0 flex-1 bg-transparent text-[19px] font-bold outline-none"
          />
          <button type="button" onClick={hide} className="label-mono text-text-quiet hover:text-text min-h-9 cursor-pointer px-1 text-[11px]">
            Esc
          </button>
        </div>
        <div ref={list} id="palette-list" role="listbox" className="min-h-0 flex-1 overflow-y-auto px-2 py-2">
          {text && search.isFetching && !search.data && <p className="label-mono text-text-quiet px-3 py-2 text-[11px]">Searching…</p>}
          {text && search.data && query === text && filterHits(search.data, filter).length === 0 && (
            <p className="text-text-quiet px-3 py-2 text-sm">Nothing matches “{text}”{filter !== "all" ? ` in ${FILTERS.find((f) => f.id === filter)!.label.toLowerCase()}` : ""}.</p>
          )}
          {groups.map((g) => (
            <div key={g.title} className="mb-1">
              <h3 className="label-mono text-text-quiet px-3 pt-2 pb-1.5 text-[11px] tracking-[0.12em]">{g.title}</h3>
              {g.items.map((item) => {
                index += 1;
                const i = index;
                return (
                  <button
                    key={item.key}
                    id={`palette-${i}`}
                    data-index={i}
                    role="option"
                    aria-selected={i === active}
                    type="button"
                    onMouseMove={() => i !== active && setActive(i)}
                    onClick={item.run}
                    className={`flex min-h-12 w-full cursor-pointer items-center gap-3 rounded-md px-3 py-2 text-left ${i === active ? "bg-surface-raised" : ""}`}
                  >
                    <span className={`label-mono w-20 shrink-0 text-[10.5px] ${item.isHit ? "text-accent" : "text-text-quiet"}`}>{item.kind}</span>
                    <span className="min-w-0 flex-1 truncate text-[16px] font-bold">{item.title}</span>
                    {item.meta && <span className="label-mono text-text-quiet hidden max-w-[40%] truncate text-[10.5px] sm:inline">{item.meta}</span>}
                    {(item.hint || i === active) && (
                      <kbd className="label-mono text-text-quiet hidden shrink-0 text-[10px] sm:inline">{i === active && item.isHit ? "Enter" : item.hint}</kbd>
                    )}
                  </button>
                );
              })}
            </div>
          ))}
        </div>
        <div className="label-mono border-rule text-text-quiet hidden items-center gap-5 border-x-0 border-b-0 px-4 py-2.5 text-[10.5px] sm:flex">
          <span>↑↓ Move</span>
          <span>Enter open</span>
          <span>
            Tab filter:{" "}
            {FILTERS.map((f, i) => (
              <span key={f.id}>
                {i > 0 && ", "}
                <button type="button" onClick={() => setFilter(f.id)} className={`cursor-pointer ${f.id === filter ? "text-text underline" : ""}`}>
                  {f.label.toLowerCase()}
                </button>
              </span>
            ))}
          </span>
        </div>
        {/* Phones have no Tab key: the filters are buttons. */}
        <div className="border-rule flex gap-1.5 overflow-x-auto border-x-0 border-b-0 px-3 py-2 sm:hidden">
          {FILTERS.map((f) => (
            <button
              key={f.id}
              type="button"
              onClick={() => setFilter(f.id)}
              className={`label-mono min-h-10 shrink-0 cursor-pointer rounded-md border px-3 text-[11px] ${f.id === filter ? "border-accent text-text" : "border-line text-text-quiet"}`}
            >
              {f.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
