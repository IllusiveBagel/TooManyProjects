// The whiteboard editor: the board under a camera you pan and zoom, the tool
// rail (or the phone's bottom bar), the style bar for what is selected, and
// typing straight onto shapes, sticky notes and arrows.
import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { uploadFiles } from "../files/queries";
import { useProjectTasks } from "../tasks/queries";
import type { BoardDoc, Changes } from "./doc";
import { copies, deletion, restack } from "./edit";
import {
  arrowEnds,
  bounds,
  canJoin,
  fromCorners,
  HANDLES,
  handlePoint,
  isBoxed,
  isShape,
  LINE,
  moved,
  newId,
  rectOf,
  resized,
  resolvedFonts,
  CSS_FONTS,
  SLOTS,
  TEXT_SIZE,
  taskChip,
  topAt,
  within,
  type ArrowEl,
  type Boxed,
  type El,
  type Fill,
  type Fonts,
  type Handle,
  type Kind,
  type PenEl,
  type Pt,
  type Rect,
  type ShapeEl,
  type Slot,
  type TextSize,
} from "./model";
import type { Cursor } from "./queries";
import { colorOf, Grid, Scene, SceneDefs, SHAPE_TEXT, STICKY_INK, STICKY_PAD, STICKY_TEXT, stickyFill } from "./Scene";

// ---------------------------------------------------------------------------
// Tools

export type Tool =
  | "select"
  | "hand"
  | "box"
  | "ellipse"
  | "diamond"
  | "arrow"
  | "line"
  | "pen"
  | "text"
  | "sticky"
  | "image"
  | "eraser";

const I = (d: ReactNode) => (
  <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    {d}
  </svg>
);
export const TOOLS: { id: Tool; label: string; key: string; icon: ReactNode }[] = [
  { id: "select", label: "Select", key: "V", icon: I(<path d="M5 3l10 6-4.5 1.2L13 16l-2 1-2.5-5.6L5 14z" fill="currentColor" stroke="none" />) },
  { id: "hand", label: "Move around", key: "H", icon: I(<path d="M6 11V5.5a1.2 1.2 0 012.4 0V10m0-5.5V4a1.2 1.2 0 012.4 0v6m0-5a1.2 1.2 0 012.4 0v5.5m0-3.5a1.2 1.2 0 012.4 0V13c0 2.8-2.2 5-5 5h-1c-1.8 0-3-1-4-2.5L4 12.5a1.2 1.2 0 012-1.3L6 11" />) },
  { id: "box", label: "Box", key: "R", icon: I(<rect x="3.5" y="5" width="13" height="10" rx="1.5" />) },
  { id: "ellipse", label: "Ellipse", key: "O", icon: I(<ellipse cx="10" cy="10" rx="7" ry="5.5" />) },
  { id: "diamond", label: "Diamond", key: "D", icon: I(<path d="M10 3l7 7-7 7-7-7z" />) },
  { id: "arrow", label: "Arrow", key: "A", icon: I(<path d="M4 16L16 4m0 0H9m7 0v7" />) },
  { id: "line", label: "Line", key: "L", icon: I(<path d="M4 16L16 4" />) },
  { id: "pen", label: "Pen", key: "P", icon: I(<path d="M3.5 16.5l1-4L13 4a1.8 1.8 0 012.6 2.6L7 15z M11.5 5.5l3 3" />) },
  { id: "text", label: "Text", key: "T", icon: I(<path d="M5 5h10M10 5v11M7.5 16h5" />) },
  { id: "sticky", label: "Sticky note", key: "S", icon: I(<path d="M4 4h12v8l-4 4H4z M12 16v-4h4" />) },
  { id: "image", label: "Photo", key: "I", icon: I(<><rect x="3" y="4.5" width="14" height="11" rx="1.5" /><circle cx="7.5" cy="8.5" r="1.3" /><path d="M3.5 14l4-3.5 3 2.5 2.5-2 3.5 3" /></>) },
  { id: "eraser", label: "Eraser", key: "E", icon: I(<path d="M8 16h8M3.8 12.2l7-7a1.5 1.5 0 012.1 0l3 3a1.5 1.5 0 010 2.1L11 15.2H7z M7.5 8.5l4.5 4.5" />) },
];
export const PHONE_TOOLS: Tool[] = ["select", "box", "arrow", "pen", "text", "sticky", "image", "eraser"];
const VIEW_TOOLS: Tool[] = ["select", "hand"];

const SLOT_NAMES: Record<Slot, string> = {
  ink: "Ink",
  "slot-1": "Colour 1",
  "slot-2": "Colour 2",
  "slot-3": "Colour 3",
  "slot-4": "Colour 4",
  "slot-5": "Colour 5",
  "slot-6": "Colour 6",
  "slot-7": "Colour 7",
  "slot-8": "Colour 8",
};

interface Style {
  color: Slot;
  fill: Fill;
  size: TextSize;
}

interface Camera {
  x: number;
  y: number;
  z: number;
}

const MIN_ZOOM = 0.1;
const MAX_ZOOM = 4;
const clampZoom = (z: number) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z));

/** What a drag is doing, kept in a ref so pointer moves don't re-render twice. */
type Gesture =
  | { type: "pan"; start: Pt; cam: Camera }
  | { type: "pinch"; d0: number; mid: Pt; cam: Camera }
  | { type: "move"; start: Pt; before: Changes; ids: string[]; moved: boolean }
  | { type: "resize"; id: string; handle: Handle; start: Pt; rect: Rect; before: Changes }
  | { type: "marquee"; a: Pt; add: string[] }
  | { type: "create"; id: string; kind: Kind; a: Pt; before: Changes }
  | { type: "arrow"; id: string; before: Changes }
  | { type: "arrowEnd"; id: string; end: "a" | "b"; before: Changes }
  | { type: "pen"; id: string; before: Changes }
  | { type: "erase"; before: Changes; gone: Set<string> }
  /** Start typing once the pointer is up, so the click doesn't take the focus away. */
  | { type: "edit"; id: string; before?: Changes };

/** Copied elements, kept across boards. */
let clipboard: El[] = [];

const inside = (p: Pt, r: Rect, pad = 0) =>
  p[0] >= r.x - pad && p[0] <= r.x + r.w + pad && p[1] >= r.y - pad && p[1] <= r.y + r.h + pad;

const typing = (t: EventTarget | null) =>
  t instanceof HTMLElement && (t.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(t.tagName));

const cursorColor = (id: string): Slot => {
  let h = 0;
  for (const ch of id) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
  return `slot-${(h % 8) + 1}` as Slot;
};

// ---------------------------------------------------------------------------

export interface EditorProps {
  doc: BoardDoc;
  rev: number;
  canEdit: boolean;
  projectId: string;
  cursors: Cursor[];
  shareCursor: (x: number, y: number) => void;
  phone?: boolean;
  /** Opens a task, from a shape's task chip. */
  onOpenTask?: (taskId: string) => void;
}

export function Editor({ doc, rev, canEdit, projectId, cursors, shareCursor, phone = false, onOpenTask }: EditorProps) {
  const pid = useId().replace(/:/g, "");
  const svgRef = useRef<SVGSVGElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const [size, setSize] = useState({ w: 800, h: 600 });
  const [cam, setCam] = useState<Camera>({ x: -100, y: -100, z: 1 });
  const camRef = useRef(cam);
  camRef.current = cam;
  const [tool, setToolState] = useState<Tool>("select");
  const [selected, setSelected] = useState<string[]>([]);
  const [editing, setEditing] = useState<{ id: string; before: Changes } | null>(null);
  const [style, setStyle] = useState<Style>({ color: "ink", fill: "tint", size: "m" });
  const [marquee, setMarquee] = useState<Rect | null>(null);
  const [snap, setSnap] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [spaceDown, setSpaceDown] = useState(false);
  const gesture = useRef<Gesture | null>(null);
  const pointers = useRef(new Map<number, Pt>());
  const lastTap = useRef<{ t: number; p: Pt } | null>(null);
  const fitted = useRef(false);
  const fonts = useMemo<Fonts>(() => resolvedFonts(), []);

  const els = useMemo(() => doc.list(), [doc, rev]); // eslint-disable-line react-hooks/exhaustive-deps
  const all = doc.map();
  const sel = useMemo(() => selected.filter((id) => all.has(id)), [selected, all, rev]); // eslint-disable-line react-hooks/exhaustive-deps

  const setTool = useCallback((t: Tool) => {
    setToolState(t);
    if (t !== "select") setSelected([]);
  }, []);

  // Board size.
  useLayoutEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;
    const ro = new ResizeObserver(() => {
      const r = svg.getBoundingClientRect();
      setSize({ w: r.width, h: r.height });
    });
    ro.observe(svg);
    return () => ro.disconnect();
  }, []);

  const fit = useCallback(
    (only?: El[]) => {
      const b = bounds(only ?? doc.list(), fonts);
      const pad = phone ? 24 : 80;
      if (!b) {
        setCam({ x: -size.w / 2, y: -size.h / 2, z: 1 });
        return;
      }
      const z = clampZoom(Math.min((size.w - pad * 2) / Math.max(b.w, 1), (size.h - pad * 2) / Math.max(b.h, 1), 1));
      setCam({ x: b.x + b.w / 2 - size.w / 2 / z, y: b.y + b.h / 2 - size.h / 2 / z, z });
    },
    [doc, fonts, size, phone],
  );

  // Fit the board once it has loaded and been measured.
  useEffect(() => {
    if (fitted.current || !doc.loaded || size.w < 50) return;
    fitted.current = true;
    fit();
  }, [doc.loaded, rev, size, fit]);

  const toWorld = useCallback((clientX: number, clientY: number): Pt => {
    const r = svgRef.current!.getBoundingClientRect();
    const c = camRef.current;
    return [c.x + (clientX - r.left) / c.z, c.y + (clientY - r.top) / c.z];
  }, []);

  const zoomAt = useCallback((factor: number, screen?: Pt) => {
    setCam((c) => {
      const r = svgRef.current?.getBoundingClientRect();
      const sx = screen ? screen[0] - (r?.left ?? 0) : (r?.width ?? 0) / 2;
      const sy = screen ? screen[1] - (r?.top ?? 0) : (r?.height ?? 0) / 2;
      const z = clampZoom(c.z * factor);
      const wx = c.x + sx / c.z,
        wy = c.y + sy / c.z;
      return { x: wx - sx / z, y: wy - sy / z, z };
    });
  }, []);

  // Wheel: scroll pans, pinch on a trackpad (or Ctrl + wheel) zooms.
  useEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      if (e.ctrlKey || e.metaKey) zoomAt(Math.exp(-e.deltaY * 0.01), [e.clientX, e.clientY]);
      else setCam((c) => ({ ...c, x: c.x + e.deltaX / c.z, y: c.y + e.deltaY / c.z }));
    };
    svg.addEventListener("wheel", onWheel, { passive: false });
    return () => svg.removeEventListener("wheel", onWheel);
  }, [zoomAt]);

  const tol = 6 / cam.z;
  const tools = canEdit ? (phone ? PHONE_TOOLS : TOOLS.map((t) => t.id)) : VIEW_TOOLS;

  // -------------------------------------------------------------------------
  // Changing things

  const change = useCallback((c: Changes) => doc.change(c), [doc]);

  const deleteSelected = useCallback(() => {
    if (!sel.length) return;
    change(deletion(doc.map(), sel, fonts));
    setSelected([]);
  }, [sel, change, doc, fonts]);

  const restyle = (patch: Partial<Style> & { dashed?: boolean; head?: boolean; hand?: boolean }) => {
    const { dashed, head, hand, ...rest } = patch;
    setStyle((s) => ({ ...s, ...rest }));
    const out: Changes = {};
    for (const id of sel) {
      const e = doc.get(id);
      if (!e) continue;
      let n: El = e;
      if (patch.color) n = { ...n, color: patch.color };
      if (patch.fill && isShape(n)) n = { ...n, fill: patch.fill };
      if (patch.size && n.kind === "text") n = { ...n, size: patch.size };
      if (hand !== undefined && n.kind === "text") n = { ...n, hand };
      if (dashed !== undefined && n.kind === "arrow") n = { ...n, dashed };
      if (head !== undefined && n.kind === "arrow") n = { ...n, head };
      out[id] = n;
    }
    change(out);
  };

  const duplicate = (src: El[], dx = 24, dy = 24) => {
    const made = copies(src, doc.map(), dx, dy, doc.nextZ(), fonts);
    const c: Changes = {};
    for (const e of made) c[e.id] = e;
    change(c);
    setSelected(made.map((e) => e.id));
  };

  const startEditing = useCallback(
    (id: string, before?: Changes) => {
      const e = doc.get(id);
      if (!e || !canEdit) return;
      doc.hold([id]);
      setSelected([id]);
      setEditing({ id, before: before ?? { [id]: e } });
    },
    [doc, canEdit],
  );

  const finishEditing = useCallback(() => {
    if (!editing) return;
    const e = doc.get(editing.id);
    doc.release();
    // An empty text box is nothing.
    if (e && e.kind === "text" && !e.text.trim()) {
      doc.live({ [e.id]: null });
      setSelected([]);
    }
    doc.record(editing.before);
    setEditing(null);
  }, [editing, doc]);

  const placePhotos = useCallback(
    async (files: File[], at?: Pt) => {
      const images = files.filter((f) => f.type.startsWith("image/"));
      if (!images.length || !canEdit) return;
      setBusy(true);
      setNote(null);
      try {
        const added = await uploadFiles({ project_id: projectId }, images);
        const c = camRef.current;
        let [x, y] = at ?? [c.x + size.w / 2 / c.z, c.y + size.h / 2 / c.z];
        const changes: Changes = {};
        let z = doc.nextZ();
        const ids: string[] = [];
        for (const a of added) {
          const w0 = a.width ?? 400,
            h0 = a.height ?? 300;
          const s = Math.min(1, 480 / Math.max(w0, h0));
          const w = Math.round(w0 * s),
            h = Math.round(h0 * s);
          const id = newId();
          changes[id] = { id, kind: "image", x: Math.round(x - w / 2), y: Math.round(y - h / 2), w, h, file: a.id, z: z++, color: "ink" };
          ids.push(id);
          x += 32;
          y += 32;
        }
        change(changes);
        setSelected(ids);
      } catch (e) {
        setNote(e instanceof Error ? e.message : "Couldn't add that photo.");
      } finally {
        setBusy(false);
      }
    },
    [canEdit, projectId, size, doc, change],
  );

  // -------------------------------------------------------------------------
  // Keys

  useEffect(() => {
    const down = (e: KeyboardEvent) => {
      if (typing(e.target) || editing) return;
      const mod = e.ctrlKey || e.metaKey;
      const k = e.key.toLowerCase();
      if (e.key === " " && !e.repeat) {
        setSpaceDown(true);
        e.preventDefault();
        return;
      }
      if (e.key === "Escape") {
        if (gesture.current) {
          cancelGesture();
          return;
        }
        setSelected([]);
        setToolState("select");
        return;
      }
      if (mod && k === "a") {
        e.preventDefault();
        setToolState("select");
        setSelected(doc.list().map((x) => x.id));
        return;
      }
      if (e.shiftKey && e.key === "!") {
        fit();
        return;
      }
      if (!mod && (e.key === "+" || e.key === "=")) return zoomAt(1.2);
      if (!mod && (e.key === "-" || e.key === "_")) return zoomAt(1 / 1.2);
      if (!canEdit) return;
      if (mod && k === "z") {
        e.preventDefault();
        if (e.shiftKey) doc.redo();
        else doc.undo();
        return;
      }
      if (mod && k === "y") {
        e.preventDefault();
        doc.redo();
        return;
      }
      if (mod && (k === "c" || k === "x") && sel.length) {
        clipboard = sel.map((id) => doc.get(id)!).filter(Boolean);
        if (k === "x") deleteSelected();
        e.preventDefault();
        return;
      }
      if (mod && k === "v" && clipboard.length) {
        // Photos pasted from outside come through the paste event instead.
        duplicate(clipboard);
        e.preventDefault();
        return;
      }
      if (mod && k === "d" && sel.length) {
        e.preventDefault();
        duplicate(sel.map((id) => doc.get(id)!));
        return;
      }
      if ((e.key === "Delete" || e.key === "Backspace") && sel.length) {
        e.preventDefault();
        deleteSelected();
        return;
      }
      if ((e.key === "]" || e.key === "[") && sel.length) {
        change(restack(doc.list(), new Set(sel), e.key === "]"));
        return;
      }
      if (e.key.startsWith("Arrow") && sel.length) {
        e.preventDefault();
        const step = e.shiftKey ? 10 : 1;
        const dx = e.key === "ArrowLeft" ? -step : e.key === "ArrowRight" ? step : 0;
        const dy = e.key === "ArrowUp" ? -step : e.key === "ArrowDown" ? step : 0;
        const c: Changes = {};
        for (const id of sel) c[id] = moved(doc.get(id)!, dx, dy);
        change(c);
        return;
      }
      if (e.key === "Enter" && sel.length === 1) {
        const el = doc.get(sel[0]!);
        if (el && el.kind !== "pen" && el.kind !== "image") {
          e.preventDefault();
          startEditing(el.id);
        }
        return;
      }
      if (!mod && !e.altKey) {
        const t = TOOLS.find((x) => x.key.toLowerCase() === k);
        if (t && tools.includes(t.id)) {
          if (t.id === "image") fileRef.current?.click();
          else setTool(t.id);
        }
      }
    };
    const up = (e: KeyboardEvent) => {
      if (e.key === " ") setSpaceDown(false);
    };
    const paste = (e: ClipboardEvent) => {
      if (typing(e.target) || !canEdit) return;
      const files = [...(e.clipboardData?.files ?? [])];
      if (files.length) {
        e.preventDefault();
        placePhotos(files);
      }
    };
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    window.addEventListener("paste", paste);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
      window.removeEventListener("paste", paste);
    };
  });

  // -------------------------------------------------------------------------
  // Pointer

  const cancelGesture = () => {
    const g = gesture.current;
    if (!g) return;
    if ("before" in g && g.before) doc.revert(g.before);
    doc.release();
    gesture.current = null;
    setMarquee(null);
    setSnap(null);
  };

  const finishGesture = () => {
    const g = gesture.current;
    gesture.current = null;
    if (!g) return;
    doc.release();
    switch (g.type) {
      case "edit":
        startEditing(g.id, g.before);
        return;
      case "marquee":
        setMarquee(null);
        return;
      case "move":
      case "resize":
      case "arrowEnd":
      case "erase":
        doc.record(g.before);
        return;
      case "pen": {
        doc.record(g.before);
        return;
      }
      case "arrow": {
        const e = doc.get(g.id) as ArrowEl | undefined;
        setSnap(null);
        if (!e) return;
        const [a, b] = arrowEnds(e, doc.map(), fonts);
        // A click, not a drag: no arrow.
        if (Math.hypot(b[0] - a[0], b[1] - a[1]) < 10 / camRef.current.z && (!e.to || e.to === e.from)) {
          doc.revert(g.before);
          return;
        }
        doc.record(g.before);
        setToolState("select");
        setSelected([g.id]);
        return;
      }
      case "create": {
        const e = doc.get(g.id);
        if (!e) return;
        // A click makes one at its usual size.
        if (isBoxed(e) && e.w < 8 && e.h < 8) {
          const w = e.kind === "sticky" ? 200 : e.kind === "diamond" ? 140 : 160;
          const h = e.kind === "sticky" ? 140 : e.kind === "diamond" ? 110 : 80;
          doc.live({ [e.id]: { ...e, x: g.a[0] - w / 2, y: g.a[1] - h / 2, w, h } });
        }
        setToolState("select");
        if (e.kind === "sticky" || e.kind === "text") startEditing(e.id, g.before);
        else {
          doc.record(g.before);
          setSelected([e.id]);
        }
        return;
      }
    }
  };

  const onPointerDown = (ev: ReactPointerEvent<SVGSVGElement>) => {
    if (editing) {
      finishEditing();
      return;
    }
    const svg = svgRef.current!;
    svg.setPointerCapture(ev.pointerId);
    pointers.current.set(ev.pointerId, [ev.clientX, ev.clientY]);

    // A second finger: pan and pinch, dropping whatever the first one started.
    if (pointers.current.size === 2) {
      cancelGesture();
      const [p1, p2] = [...pointers.current.values()] as [Pt, Pt];
      gesture.current = {
        type: "pinch",
        d0: Math.hypot(p2[0] - p1[0], p2[1] - p1[1]),
        mid: toWorld((p1[0] + p2[0]) / 2, (p1[1] + p2[1]) / 2),
        cam: camRef.current,
      };
      return;
    }
    if (pointers.current.size > 2) return;

    const p = toWorld(ev.clientX, ev.clientY);
    const touch = ev.pointerType === "touch";
    const pan = () => {
      gesture.current = { type: "pan", start: [ev.clientX, ev.clientY], cam: camRef.current };
    };
    if (ev.button === 1 || tool === "hand" || spaceDown) return pan();
    if (ev.button !== 0) return;

    // Double tap or double click.
    const now = Date.now();
    const lt = lastTap.current;
    const double = !!lt && now - lt.t < 350 && Math.hypot(ev.clientX - lt.p[0], ev.clientY - lt.p[1]) < 16;
    lastTap.current = double ? null : { t: now, p: [ev.clientX, ev.clientY] };

    const hold = (ids: string[]) => doc.hold(ids);
    const snapshot = (ids: string[]) => {
      const b: Changes = {};
      for (const id of ids) b[id] = doc.get(id) ?? null;
      return b;
    };

    // A task chip opens its task, for viewers too.
    if (tool === "select" || !canEdit) {
      const chip = els.findLast((e) => isShape(e) && e.task && inside(p, taskChip(e as Boxed), 2 / camRef.current.z));
      if (chip && isShape(chip) && chip.task && onOpenTask) {
        onOpenTask(chip.task);
        return;
      }
    }

    if (tool === "select") {
      if (!canEdit) return pan();
      const z = camRef.current.z;
      // A handle of the one selected thing.
      if (sel.length === 1) {
        const e = doc.get(sel[0]!)!;
        if (isBoxed(e)) {
          const r = rectOf(e, fonts);
          for (const h of HANDLES) {
            const [hx, hy] = handlePoint(r, h, 8 / z);
            if (Math.abs(p[0] - hx) < (touch ? 16 : 8) / z && Math.abs(p[1] - hy) < (touch ? 16 : 8) / z) {
              hold([e.id]);
              gesture.current = { type: "resize", id: e.id, handle: h, start: p, rect: r, before: snapshot([e.id]) };
              return;
            }
          }
        }
        if (e.kind === "arrow") {
          const [a, b] = arrowEnds(e, doc.map(), fonts);
          for (const [end, q] of [["a", a], ["b", b]] as const) {
            if (Math.hypot(p[0] - q[0], p[1] - q[1]) < (touch ? 18 : 9) / z) {
              hold([e.id]);
              gesture.current = { type: "arrowEnd", id: e.id, end, before: snapshot([e.id]) };
              return;
            }
          }
        }
      }
      const hit = topAt(els, p, tol, fonts);
      if (double) {
        if (hit && hit.kind !== "pen" && hit.kind !== "image") {
          gesture.current = { type: "edit", id: hit.id };
          return;
        }
        if (!hit) {
          const id = newId();
          doc.live({ [id]: { id, kind: "text", x: p[0], y: p[1] - (TEXT_SIZE[style.size] * LINE) / 2, text: "", size: style.size, color: style.color, z: doc.nextZ() } });
          gesture.current = { type: "edit", id, before: { [id]: null } };
          return;
        }
      }
      if (hit) {
        let ids = sel;
        if (ev.shiftKey) ids = sel.includes(hit.id) ? sel.filter((x) => x !== hit.id) : [...sel, hit.id];
        else if (!sel.includes(hit.id)) ids = [hit.id];
        setSelected(ids);
        if (!ids.includes(hit.id)) return;
        hold(ids);
        gesture.current = { type: "move", start: p, before: snapshot(ids), ids, moved: false };
        return;
      }
      if (!ev.shiftKey) setSelected([]);
      // On a phone, a finger on empty board moves around.
      if (touch) return pan();
      gesture.current = { type: "marquee", a: p, add: ev.shiftKey ? sel : [] };
      setMarquee({ x: p[0], y: p[1], w: 0, h: 0 });
      return;
    }
    if (!canEdit) return pan();

    const z0 = doc.nextZ();
    const id = newId();
    switch (tool) {
      case "box":
      case "ellipse":
      case "diamond":
      case "sticky": {
        const e: El =
          tool === "sticky"
            ? { id, kind: "sticky", x: p[0], y: p[1], w: 0, h: 0, text: "", color: style.color === "ink" ? "slot-6" : style.color, z: z0 }
            : { id, kind: tool, x: p[0], y: p[1], w: 0, h: 0, fill: style.fill, color: style.color, z: z0 };
        doc.live({ [id]: e });
        hold([id]);
        gesture.current = { type: "create", id, kind: tool, a: p, before: { [id]: null } };
        return;
      }
      case "text": {
        doc.live({ [id]: { id, kind: "text", x: p[0], y: p[1] - (TEXT_SIZE[style.size] * LINE) / 2, text: "", size: style.size, color: style.color, z: z0 } });
        setToolState("select");
        gesture.current = { type: "edit", id, before: { [id]: null } };
        return;
      }
      case "arrow":
      case "line": {
        const from = topAt(els, p, tol, fonts, canJoin);
        const e: ArrowEl = { id, kind: "arrow", a: p, b: p, from: from?.id, color: style.color, head: tool === "arrow", z: z0 };
        if (!from) delete e.from;
        doc.live({ [id]: e });
        hold([id]);
        gesture.current = { type: "arrow", id, before: { [id]: null } };
        return;
      }
      case "pen": {
        const e: PenEl = { id, kind: "pen", points: [p], width: 3, color: style.color, z: z0 };
        doc.live({ [id]: e });
        hold([id]);
        gesture.current = { type: "pen", id, before: { [id]: null } };
        return;
      }
      case "eraser": {
        gesture.current = { type: "erase", before: {}, gone: new Set() };
        erase(p);
        return;
      }
      case "image":
        fileRef.current?.click();
        return;
    }
  };

  const erase = (p: Pt) => {
    const g = gesture.current;
    if (g?.type !== "erase") return;
    const hit = topAt(doc.list(), p, tol * 1.5, fonts);
    if (!hit) return;
    const c = deletion(doc.map(), [hit.id], fonts);
    for (const id of Object.keys(c)) if (!(id in g.before)) g.before[id] = doc.get(id) ?? null;
    g.gone.add(hit.id);
    doc.live(c);
  };

  const onPointerMove = (ev: ReactPointerEvent<SVGSVGElement>) => {
    if (ev.pointerType === "mouse" || ev.pointerType === "pen") {
      const [x, y] = toWorld(ev.clientX, ev.clientY);
      shareCursor(Math.round(x), Math.round(y));
    }
    if (!pointers.current.has(ev.pointerId)) return;
    pointers.current.set(ev.pointerId, [ev.clientX, ev.clientY]);
    const g = gesture.current;
    if (!g) return;
    const p = toWorld(ev.clientX, ev.clientY);
    switch (g.type) {
      case "pan":
        setCam({ ...g.cam, x: g.cam.x - (ev.clientX - g.start[0]) / g.cam.z, y: g.cam.y - (ev.clientY - g.start[1]) / g.cam.z });
        return;
      case "pinch": {
        if (pointers.current.size < 2) return;
        const [p1, p2] = [...pointers.current.values()] as [Pt, Pt];
        const d = Math.hypot(p2[0] - p1[0], p2[1] - p1[1]);
        const z = clampZoom(g.cam.z * (d / Math.max(g.d0, 1)));
        const r = svgRef.current!.getBoundingClientRect();
        const mx = (p1[0] + p2[0]) / 2 - r.left,
          my = (p1[1] + p2[1]) / 2 - r.top;
        setCam({ x: g.mid[0] - mx / z, y: g.mid[1] - my / z, z });
        return;
      }
      case "move": {
        const dx = p[0] - g.start[0],
          dy = p[1] - g.start[1];
        if (!g.moved && Math.hypot(dx, dy) * camRef.current.z < 3) return;
        g.moved = true;
        const c: Changes = {};
        for (const id of g.ids) {
          const was = g.before[id];
          if (was) c[id] = moved(was, Math.round(dx), Math.round(dy));
        }
        doc.live(c);
        return;
      }
      case "resize": {
        const was = g.before[g.id] as Boxed;
        const r = resized(g.rect, g.handle, p[0] - g.start[0], p[1] - g.start[1], ev.shiftKey || was.kind === "image");
        doc.live({ [g.id]: { ...was, x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.w), h: Math.round(r.h) } });
        return;
      }
      case "marquee": {
        const r = fromCorners(g.a, p);
        setMarquee(r);
        const inside = within(doc.list(), r, fonts).map((e) => e.id);
        setSelected([...new Set([...g.add, ...inside])]);
        return;
      }
      case "create": {
        const e = doc.get(g.id);
        if (!e || !isBoxed(e)) return;
        let r = fromCorners(g.a, p);
        if (ev.shiftKey) {
          const s = Math.max(r.w, r.h);
          r = fromCorners(g.a, [g.a[0] + Math.sign(p[0] - g.a[0] || 1) * s, g.a[1] + Math.sign(p[1] - g.a[1] || 1) * s]);
        }
        doc.live({ [g.id]: { ...e, x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.w), h: Math.round(r.h) } });
        return;
      }
      case "arrow":
      case "arrowEnd": {
        const e = doc.get(g.id) as ArrowEl | undefined;
        if (!e) return;
        const end = g.type === "arrow" ? "b" : g.end;
        const other = end === "b" ? e.from : e.to;
        const target = topAt(doc.list(), p, tol, fonts, (x) => canJoin(x) && x.id !== other);
        setSnap(target?.id ?? null);
        const n: ArrowEl = { ...e, [end]: p };
        if (end === "b") {
          if (target) n.to = target.id;
          else delete n.to;
        } else if (target) n.from = target.id;
        else delete n.from;
        doc.live({ [g.id]: n });
        return;
      }
      case "pen": {
        const e = doc.get(g.id) as PenEl | undefined;
        if (!e) return;
        const lastP = e.points[e.points.length - 1]!;
        if (Math.hypot(p[0] - lastP[0], p[1] - lastP[1]) < 1.5 / camRef.current.z) return;
        doc.live({ [g.id]: { ...e, points: [...e.points, [Math.round(p[0] * 10) / 10, Math.round(p[1] * 10) / 10]] } });
        return;
      }
      case "erase":
        erase(p);
        return;
    }
  };

  const onPointerUp = (ev: ReactPointerEvent<SVGSVGElement>) => {
    pointers.current.delete(ev.pointerId);
    const g = gesture.current;
    if (g?.type === "pinch") {
      // One finger left: carry on moving around with it.
      if (pointers.current.size === 1) {
        const [q] = [...pointers.current.values()] as [Pt];
        gesture.current = { type: "pan", start: q, cam: camRef.current };
      } else gesture.current = null;
      return;
    }
    if (ev.type === "pointercancel") cancelGesture();
    else finishGesture();
  };

  // -------------------------------------------------------------------------
  // Drawing

  const view: Rect = { x: cam.x, y: cam.y, w: size.w / cam.z, h: size.h / cam.z };
  const editingEl = editing ? doc.get(editing.id) : undefined;
  const hide = useMemo(() => (editing ? new Set([editing.id]) : undefined), [editing]);
  const drawing = gesture.current?.type === "arrow" || gesture.current?.type === "arrowEnd";
  const selEls = sel.map((id) => all.get(id)!).filter(Boolean);
  const cursor = tool === "hand" || spaceDown || !canEdit ? "grab" : tool === "select" ? "default" : "crosshair";

  return (
    <div
      className="relative min-h-0 flex-1 overflow-hidden"
      onDragOver={(e) => canEdit && e.preventDefault()}
      onDrop={(e) => {
        if (!canEdit) return;
        e.preventDefault();
        placePhotos([...e.dataTransfer.files], toWorld(e.clientX, e.clientY));
      }}
    >
      <svg
        ref={svgRef}
        className="bg-surface block h-full w-full touch-none select-none"
        style={{ cursor }}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        onContextMenu={(e) => e.preventDefault()}
        role="application"
        aria-label="Whiteboard"
      >
        <SceneDefs pid={pid} />
        <g transform={`scale(${cam.z}) translate(${-cam.x} ${-cam.y})`}>
          <Grid pid={pid} r={view} />
          <Scene els={els} measure={fonts} hide={hide} pid={pid} />
          <SelectionLayer els={selEls} all={all} fonts={fonts} z={cam.z} handles={canEdit && !drawing && !editing} />
          {snap && all.get(snap) && <Snap r={rectOf(all.get(snap)!, fonts)} z={cam.z} />}
          {marquee && <rect x={marquee.x} y={marquee.y} width={marquee.w} height={marquee.h} fill="color-mix(in srgb, var(--tmp-accent) 8%, transparent)" stroke="var(--tmp-accent)" strokeWidth={1 / cam.z} strokeDasharray={`${4 / cam.z} ${3 / cam.z}`} />}
          {cursors.map((c) => (
            <OtherCursor key={c.user_id} x={c.x} y={c.y} name={c.name} color={cursorColor(c.user_id)} z={cam.z} />
          ))}
        </g>
      </svg>

      {editingEl && <TextEditor el={editingEl} cam={cam} doc={doc} fonts={fonts} onDone={finishEditing} />}

      <input
        ref={fileRef}
        type="file"
        accept="image/*"
        multiple
        hidden
        onChange={(e) => {
          placePhotos([...(e.target.files ?? [])]);
          e.target.value = "";
          setToolState("select");
        }}
      />

      {/* Tools */}
      {!phone && (
        <div className="border-rule bg-surface-card shadow-card absolute top-4 left-4 flex flex-col gap-0.5 rounded-lg p-1" role="toolbar" aria-label="Tools" aria-orientation="vertical">
          {TOOLS.filter((t) => tools.includes(t.id)).map((t, i) => (
            <div key={t.id}>
              {canEdit && (i === 2 || i === 7 || i === 10) && <div className="bg-line mx-1.5 my-1 h-px" />}
              <ToolButton t={t} on={t.id === tool} onPick={() => (t.id === "image" ? fileRef.current?.click() : setTool(t.id))} />
            </div>
          ))}
        </div>
      )}

      {canEdit && selEls.length > 0 && !editing && !drawing && (
        <div className={phone ? "absolute inset-x-3 bottom-3" : "absolute top-4 left-1/2 max-w-[calc(100%-120px)] -translate-x-1/2"}>
          <StyleBar
            els={selEls}
            stacked={phone}
            onStyle={restyle}
            onDelete={deleteSelected}
            onLabel={(id) => startEditing(id)}
            projectId={projectId}
            onTask={(id, task) => {
              const e = doc.get(id);
              if (!e || !isShape(e)) return;
              const n = { ...e };
              if (task) {
                n.task = task.id;
                n.taskRef = `T-${task.number}`;
              } else {
                delete n.task;
                delete n.taskRef;
              }
              change({ [id]: n });
            }}
            onOpenTask={onOpenTask}
          />
        </div>
      )}

      {drawing && (
        <div className="label-mono border-rule bg-surface-card shadow-card text-text-quiet pointer-events-none absolute bottom-4 left-1/2 -translate-x-1/2 rounded-md px-3 py-2 text-[11px] whitespace-nowrap">
          Let go on a shape to join it{phone ? "" : " · Esc cancels"}
        </div>
      )}
      {(busy || note) && (
        <div role="status" className="label-mono border-rule bg-surface-card shadow-card absolute top-4 right-4 max-w-72 rounded-md px-3 py-2 text-[11px]">
          {busy ? "Adding photo…" : note}
          {note && (
            <button type="button" className="text-text-quiet ml-2 cursor-pointer" onClick={() => setNote(null)} aria-label="Dismiss">
              ×
            </button>
          )}
        </div>
      )}

      {!phone && (
        <>
          <div className="border-rule bg-surface-card shadow-card label-mono absolute bottom-4 left-4 flex items-center rounded-lg p-1 text-[11px]">
            <IconButton label="Zoom out (−)" onClick={() => zoomAt(1 / 1.2)}>−</IconButton>
            <button type="button" className="text-text-quiet hover:text-text w-12 cursor-pointer text-center" title="Back to 100%" onClick={() => zoomAt(1 / cam.z)}>
              {Math.round(cam.z * 100)}%
            </button>
            <IconButton label="Zoom in (+)" onClick={() => zoomAt(1.2)}>+</IconButton>
            <span className="bg-line mx-1 h-6 w-px" />
            <button type="button" className="text-text-quiet hover:text-text min-h-9 cursor-pointer rounded-md px-2" title="Fit the board (Shift+1)" onClick={() => fit()}>
              Fit
            </button>
          </div>
          {canEdit && (
            <div className="border-rule bg-surface-card shadow-card absolute right-4 bottom-4 flex rounded-lg p-1">
              <IconButton label="Undo (Ctrl+Z)" disabled={!doc.canUndo} onClick={() => doc.undo()}>↶</IconButton>
              <IconButton label="Redo (Ctrl+Shift+Z)" disabled={!doc.canRedo} onClick={() => doc.redo()}>↷</IconButton>
            </div>
          )}
        </>
      )}

      {phone && (
        <PhoneTools tools={tools} tool={tool} onPick={(t) => (t === "image" ? fileRef.current?.click() : setTool(t))} onFit={() => fit()} />
      )}
      {!canEdit && (
        <div className="label-mono border-rule bg-surface-card text-text-quiet pointer-events-none absolute top-4 right-4 rounded-md px-2.5 py-1.5 text-[10px]">
          View only
        </div>
      )}
      {!els.length && doc.loaded && !editing && (
        <p className="font-reading text-text-quiet pointer-events-none absolute top-1/2 left-1/2 w-72 -translate-x-1/2 -translate-y-1/2 text-center text-[17px]">
          {canEdit ? (phone ? "Pick a tool below and draw with one finger. Two fingers move around." : "Pick a tool on the left, or press R for a box, A for an arrow, P for the pen.") : "Nothing on this board yet."}
        </p>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Overlays

function SelectionLayer({ els, all, fonts, z, handles }: { els: El[]; all: Map<string, El>; fonts: Fonts; z: number; handles: boolean }) {
  const single = els.length === 1 ? els[0]! : null;
  return (
    <g pointerEvents="none">
      {els.map((e) => {
        if (e.kind === "arrow") {
          const [a, b] = arrowEnds(e, all, fonts);
          return (
            <g key={e.id}>
              <line x1={a[0]} y1={a[1]} x2={b[0]} y2={b[1]} stroke="var(--tmp-accent)" strokeWidth={6 / z} strokeOpacity={0.25} strokeLinecap="round" />
              {handles &&
                [a, b].map((q, i) => (
                  <circle key={i} cx={q[0]} cy={q[1]} r={6 / z} fill="var(--tmp-surface)" stroke="var(--tmp-accent)" strokeWidth={2 / z} />
                ))}
            </g>
          );
        }
        const r = rectOf(e, fonts);
        const p = 8 / z;
        return (
          <g key={e.id}>
            <rect x={r.x - p} y={r.y - p} width={r.w + p * 2} height={r.h + p * 2} fill="none" stroke="var(--tmp-accent)" strokeWidth={1.5 / z} strokeDasharray={`${5 / z} ${4 / z}`} />
            {handles &&
              single === e &&
              isBoxed(e) &&
              HANDLES.map((h) => {
                const [hx, hy] = handlePoint(r, h, p);
                return <rect key={h} x={hx - 5 / z} y={hy - 5 / z} width={10 / z} height={10 / z} rx={2 / z} fill="var(--tmp-surface)" stroke="var(--tmp-accent)" strokeWidth={2 / z} />;
              })}
          </g>
        );
      })}
    </g>
  );
}

function Snap({ r, z }: { r: Rect; z: number }) {
  const p = 6 / z;
  return (
    <g pointerEvents="none">
      <rect x={r.x - p} y={r.y - p} width={r.w + p * 2} height={r.h + p * 2} rx={9 / z} fill="none" stroke="var(--tmp-accent)" strokeWidth={2.5 / z} />
      {(
        [
          [r.x + r.w / 2, r.y],
          [r.x + r.w, r.y + r.h / 2],
          [r.x + r.w / 2, r.y + r.h],
          [r.x, r.y + r.h / 2],
        ] as Pt[]
      ).map(([cx, cy], i) => (
        <circle key={i} cx={cx} cy={cy} r={5 / z} fill="var(--tmp-accent)" />
      ))}
    </g>
  );
}

function OtherCursor({ x, y, name, color, z }: { x: number; y: number; name: string; color: Slot; z: number }) {
  return (
    <g transform={`translate(${x} ${y}) scale(${1 / z})`} pointerEvents="none" style={{ transition: "transform 0.25s linear" }}>
      <path d="M0,0 L0,20 L5,15 L9,24 L12,23 L8,14 L15,14 Z" fill={colorOf(color)} stroke="var(--tmp-surface)" strokeWidth={1.5} />
      <rect x={14} y={20} width={name.length * 7.5 + 14} height={20} rx={3} fill={colorOf(color)} />
      <text x={21} y={34} fill="var(--tmp-on-accent)" style={{ font: "700 11px var(--tmp-font-mono)" }}>
        {name}
      </text>
    </g>
  );
}

/** Typing onto the board: a textarea laid exactly over the element. */
function TextEditor({ el, cam, doc, fonts, onDone }: { el: El; cam: Camera; doc: BoardDoc; fonts: Fonts; onDone: () => void }) {
  const ref = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    const t = ref.current;
    if (!t) return;
    t.focus();
    t.select();
  }, []);
  const z = cam.z;
  const sx = (x: number) => (x - cam.x) * z;
  const sy = (y: number) => (y - cam.y) * z;
  let value = "";
  let style: CSSProperties = {};
  const base: CSSProperties = { position: "absolute", background: "transparent", border: 0, outline: "none", resize: "none", overflow: "hidden", padding: 0, margin: 0, lineHeight: LINE };
  const set = (text: string) => {
    if (el.kind === "arrow") doc.live({ [el.id]: { ...el, label: text } });
    else if (el.kind === "box" || el.kind === "ellipse" || el.kind === "diamond") doc.live({ [el.id]: { ...el, text } });
    else if (el.kind === "sticky" || el.kind === "text") doc.live({ [el.id]: { ...el, text } });
  };
  switch (el.kind) {
    case "text": {
      value = el.text;
      const size = TEXT_SIZE[el.size];
      const r = rectOf(el, fonts);
      style = { ...base, left: sx(el.x), top: sy(el.y), width: (r.w + size * 2) * z, height: r.h * z, font: CSS_FONTS.text(el), fontSize: size * z, color: colorOf(el.color), whiteSpace: "pre" };
      break;
    }
    case "sticky":
      value = el.text;
      style = { ...base, left: sx(el.x), top: sy(el.y), width: el.w * z, height: el.h * z, padding: `${10 * z}px ${STICKY_PAD * z}px`, font: CSS_FONTS.sticky, fontSize: STICKY_TEXT * z, color: STICKY_INK, background: stickyFill(el.color), borderRadius: 3 * z };
      break;
    case "box":
    case "ellipse":
    case "diamond": {
      value = el.text ?? "";
      const lines = Math.max(1, value.split("\n").length);
      const h = lines * SHAPE_TEXT * LINE;
      const pad = el.kind === "box" ? 8 : el.w * 0.15;
      style = { ...base, left: sx(el.x + pad), top: sy(el.y + el.h / 2 - h / 2), width: (el.w - pad * 2) * z, height: h * z + 4, font: CSS_FONTS.shape, fontSize: SHAPE_TEXT * z, textAlign: "center", color: el.fill === "solid" ? "var(--tmp-on-accent)" : "var(--tmp-text)" };
      break;
    }
    case "arrow": {
      value = el.label ?? "";
      const [a, b] = arrowEnds(el, doc.map(), fonts);
      const mx = (a[0] + b[0]) / 2,
        my = (a[1] + b[1]) / 2;
      const w = Math.max(80, value.length * 9 + 24);
      style = { ...base, left: sx(mx - w / 2), top: sy(my - 12), width: w * z, height: 24 * z, font: CSS_FONTS.label, fontSize: 12 * z, textAlign: "center", textTransform: "uppercase", letterSpacing: "0.06em", background: "var(--tmp-surface)", border: `${1.5 * z}px solid var(--tmp-accent)`, borderRadius: 3 * z, color: "var(--tmp-text)", paddingTop: 4 * z };
      break;
    }
    default:
      return null;
  }
  return (
    <textarea
      ref={ref}
      value={value}
      aria-label={el.kind === "arrow" ? "Arrow label" : "Text"}
      spellCheck
      style={style}
      onChange={(e) => set(el.kind === "arrow" ? e.target.value.replace(/\n/g, " ") : e.target.value)}
      onBlur={onDone}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Escape" || (e.key === "Enter" && (e.metaKey || e.ctrlKey || el.kind === "arrow"))) {
          e.preventDefault();
          onDone();
        }
      }}
    />
  );
}

// ---------------------------------------------------------------------------
// Controls

function IconButton({ label, children, onClick, disabled }: { label: string; children: ReactNode; onClick?: () => void; disabled?: boolean }) {
  return (
    <button type="button" aria-label={label} title={label} disabled={disabled} onClick={onClick} className="text-text hover:bg-surface-raised grid min-h-9 min-w-9 cursor-pointer place-items-center rounded-md text-[17px] disabled:cursor-default disabled:opacity-35">
      {children}
    </button>
  );
}

function ToolButton({ t, on, big = false, onPick }: { t: (typeof TOOLS)[number]; on: boolean; big?: boolean; onPick: () => void }) {
  return (
    <button
      type="button"
      aria-pressed={on}
      aria-label={t.label}
      title={big ? t.label : `${t.label} (${t.key})`}
      onClick={onPick}
      className={`relative grid shrink-0 cursor-pointer place-items-center rounded-md ${big ? "size-11" : "size-10"} ${on ? "bg-accent text-on-accent" : "text-text hover:bg-surface-raised"}`}
    >
      {t.icon}
      {!big && <span className={`label-mono absolute right-1 bottom-0.5 text-[8px] ${on ? "opacity-70" : "text-text-quiet"}`}>{t.key}</span>}
    </button>
  );
}

function PhoneTools({ tools, tool, onPick, onFit }: { tools: Tool[]; tool: Tool; onPick: (t: Tool) => void; onFit: () => void }) {
  return (
    <div className="border-rule bg-surface-card absolute inset-x-0 bottom-0 flex items-center gap-1 overflow-x-auto border-x-0 border-b-0 px-2 pt-1.5 pb-[calc(env(safe-area-inset-bottom)+6px)]" role="toolbar" aria-label="Tools">
      {TOOLS.filter((t) => tools.includes(t.id)).map((t) => (
        <ToolButton key={t.id} t={t} on={t.id === tool} big onPick={() => onPick(t.id)} />
      ))}
      <span className="flex-1" />
      <button type="button" className="label-mono text-text-quiet min-h-11 shrink-0 cursor-pointer px-2 text-[11px]" onClick={onFit}>
        Fit
      </button>
    </div>
  );
}

/** Colour, fill and the rest for what is selected. */
function StyleBar({
  els,
  stacked = false,
  onStyle,
  onDelete,
  onLabel,
  projectId,
  onTask,
  onOpenTask,
}: {
  els: El[];
  stacked?: boolean;
  onStyle: (p: Partial<Style> & { dashed?: boolean; head?: boolean; hand?: boolean }) => void;
  onDelete: () => void;
  onLabel: (id: string) => void;
  projectId: string;
  onTask: (id: string, task: { id: string; number: number } | null) => void;
  onOpenTask?: (taskId: string) => void;
}) {
  const btn = "label-mono text-text hover:bg-surface-raised min-h-10 shrink-0 cursor-pointer rounded-md px-2.5 text-[11px]";
  const seg = (on: boolean) => `label-mono min-h-8 cursor-pointer rounded-sm px-2 text-[10px] ${on ? "bg-accent text-on-accent" : "text-text-quiet hover:text-text"}`;
  const first = els[0]!;
  const shapes = els.filter(isShape);
  const texts = els.filter((e) => e.kind === "text");
  const arrows = els.filter((e): e is ArrowEl => e.kind === "arrow");
  const colourable = els.some((e) => e.kind !== "image");
  const color = first.color;
  const rule = <span className="bg-line mx-0.5 h-6 w-px shrink-0" aria-hidden="true" />;

  const swatches = colourable && (
    <div className={`flex items-center ${stacked ? "justify-between" : ""}`} role="radiogroup" aria-label="Colour">
      {SLOTS.map((c) => (
        <button key={c} type="button" role="radio" aria-checked={c === color} aria-label={SLOT_NAMES[c]} title={SLOT_NAMES[c]} onClick={() => onStyle({ color: c })} className={`grid ${stacked ? "size-9" : "size-8"} shrink-0 cursor-pointer place-items-center rounded-md border-2 ${c === color ? "border-text" : "border-transparent"}`}>
          <span className="size-4.5 rounded-sm" style={{ background: first.kind === "sticky" ? stickyFill(c) : colorOf(c) }} />
        </button>
      ))}
    </div>
  );
  const fill = shapes.length > 0 && (
    <div className="border-rule flex shrink-0 rounded-md p-0.5" role="radiogroup" aria-label="Fill">
      {(["none", "tint", "solid"] as const).map((f) => (
        <button key={f} type="button" role="radio" aria-checked={shapes[0]!.fill === f} className={seg(shapes[0]!.fill === f)} onClick={() => onStyle({ fill: f })}>
          {f === "none" ? "None" : f === "tint" ? "Tint" : "Solid"}
        </button>
      ))}
    </div>
  );
  const sizes = texts.length > 0 && (
    <div className="border-rule flex shrink-0 rounded-md p-0.5" role="radiogroup" aria-label="Text size">
      {(["s", "m", "l", "xl"] as const).map((s) => {
        const on = texts[0]!.kind === "text" && texts[0]!.size === s;
        return (
          <button key={s} type="button" role="radio" aria-checked={on} className={seg(on)} onClick={() => onStyle({ size: s })}>
            {s.toUpperCase()}
          </button>
        );
      })}
      <button type="button" aria-pressed={texts[0]!.kind === "text" && !!texts[0]!.hand} className={seg(texts[0]!.kind === "text" && !!texts[0]!.hand)} title="Handwritten look" onClick={() => onStyle({ hand: !(texts[0]!.kind === "text" && texts[0]!.hand) })}>
        Hand
      </button>
    </div>
  );
  const arrowBits = arrows.length > 0 && (
    <div className="flex shrink-0 items-center gap-0.5">
      <div className="border-rule flex rounded-md p-0.5">
        <button type="button" aria-pressed={!!arrows[0]!.dashed} className={seg(!!arrows[0]!.dashed)} onClick={() => onStyle({ dashed: !arrows[0]!.dashed })}>
          Dashed
        </button>
        <button type="button" aria-pressed={arrows[0]!.head} className={seg(arrows[0]!.head)} onClick={() => onStyle({ head: !arrows[0]!.head })}>
          Head
        </button>
      </div>
      {arrows.length === 1 && els.length === 1 && (
        <button type="button" className={btn} onClick={() => onLabel(arrows[0]!.id)}>
          Label
        </button>
      )}
    </div>
  );
  const remove = (
    <button type="button" className={`${btn} text-late`} onClick={onDelete}>
      Delete
    </button>
  );
  const shape = els.length === 1 && isShape(first) ? first : null;
  const task = shape && <TaskButton shape={shape} projectId={projectId} btn={btn} onTask={(t) => onTask(shape.id, t)} onOpen={onOpenTask} up={stacked} />;
  const name =
    els.length > 1
      ? `${els.length} selected`
      : first.kind === "text" || first.kind === "sticky"
        ? first.text.split("\n")[0] || KIND_NAME[first.kind]
        : isShape(first)
          ? first.text?.split("\n")[0] || KIND_NAME[first.kind]
          : KIND_NAME[first.kind];

  if (stacked)
    return (
      <div className="border-rule bg-surface-card shadow-card mb-16 flex flex-col gap-1.5 rounded-lg p-2">
        <div className="flex items-center justify-between gap-2">
          <span className="label-mono text-text-quiet min-w-0 truncate px-1 text-[10px]">{name}</span>
          {fill || sizes || arrowBits}
        </div>
        {swatches}
        <div className="flex items-center justify-between gap-2">
          {task ?? <span />}
          {remove}
        </div>
      </div>
    );
  return (
    <div className="border-rule bg-surface-card shadow-card flex flex-wrap items-center gap-1 rounded-lg p-1">
      {swatches}
      {fill && (
        <>
          {rule}
          {fill}
        </>
      )}
      {sizes && (
        <>
          {rule}
          {sizes}
        </>
      )}
      {arrowBits && (
        <>
          {rule}
          {arrowBits}
        </>
      )}
      {task && (
        <>
          {rule}
          {task}
        </>
      )}
      {rule}
      {remove}
    </div>
  );
}

/** Links the selected shape to one of the project's tasks, or shows the link. */
function TaskButton({
  shape,
  projectId,
  btn,
  onTask,
  onOpen,
  up,
}: {
  shape: ShapeEl;
  projectId: string;
  btn: string;
  onTask: (t: { id: string; number: number } | null) => void;
  onOpen?: (taskId: string) => void;
  up: boolean;
}) {
  const [picking, setPicking] = useState(false);
  const [q, setQ] = useState("");
  const tasks = useProjectTasks(projectId).data ?? [];
  const linked = shape.task ? tasks.find((t) => t.id === shape.task) : undefined;
  const words = q.trim().toLowerCase();
  const shown = tasks
    .filter((t) => !words || t.title.toLowerCase().includes(words) || `t-${t.number}`.startsWith(words))
    .sort((a, b) => Number(a.category === "done") - Number(b.category === "done") || b.number - a.number)
    .slice(0, 8);
  if (shape.task)
    return (
      <span className="flex shrink-0 items-center">
        <button type="button" className={`${btn} flex items-center gap-1.5`} onClick={() => onOpen?.(shape.task!)} title="Open the task">
          <span className="bg-accent text-on-accent rounded-sm px-1.5 py-0.5 text-[10px]">{shape.taskRef}</span>
          <span className="max-w-40 truncate normal-case">{linked?.title ?? "Task"}</span>
        </button>
        <button type="button" aria-label="Unlink the task" title="Unlink the task" className="text-text-quiet hover:text-text min-h-10 cursor-pointer px-1.5" onClick={() => onTask(null)}>
          ×
        </button>
      </span>
    );
  return (
    <span className="relative shrink-0">
      <button type="button" className={btn} aria-expanded={picking} onClick={() => setPicking((p) => !p)}>
        Link task
      </button>
      {picking && (
        <div className={`border-rule bg-surface-raised shadow-card absolute z-30 flex w-72 flex-col gap-1 rounded-lg p-1.5 ${up ? "bottom-11 left-0" : "top-11 left-0"}`}>
          <input
            autoFocus
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={(e) => {
              e.stopPropagation();
              if (e.key === "Escape") setPicking(false);
              if (e.key === "Enter" && shown[0]) {
                onTask(shown[0]);
                setPicking(false);
              }
            }}
            placeholder="Find a task"
            aria-label="Find a task"
            className="border-rule bg-surface text-text min-h-9 rounded-md px-2.5 font-mono text-[12px] outline-none"
          />
          {shown.map((t) => (
            <button
              key={t.id}
              type="button"
              className="hover:bg-line flex min-h-9 cursor-pointer items-center gap-2 rounded-sm px-2 text-left text-sm"
              onClick={() => {
                onTask(t);
                setPicking(false);
              }}
            >
              <span className="label-mono text-text-quiet w-11 shrink-0 text-[10px]">T-{t.number}</span>
              <span className={`truncate ${t.category === "done" ? "text-text-quiet line-through" : ""}`}>{t.title}</span>
            </button>
          ))}
          {!shown.length && <span className="label-mono text-text-quiet px-2 py-2 text-[10px]">No tasks match</span>}
        </div>
      )}
    </span>
  );
}

const KIND_NAME: Record<Kind, string> = {
  box: "Box",
  ellipse: "Ellipse",
  diamond: "Diamond",
  sticky: "Sticky note",
  text: "Text",
  arrow: "Arrow",
  pen: "Drawing",
  image: "Photo",
};

