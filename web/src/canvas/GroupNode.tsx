import { NodeResizer, NodeToolbar, Position, useStore, type Node, type NodeProps, type ReactFlowState } from "@xyflow/react";
import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import { createPortal } from "react-dom";
import { projectColor } from "../theme/resolve";
import { cardCount, MIN_GROUP, type Rect } from "./groups";
import { CARD_WIDTH } from "./ProjectNode";
import { useCanvasUi } from "./store";

export type GroupNodeData = {
  label: string;
  color: string;
  onRename: (label: string) => void;
  onRecolor: (color: string) => void;
  onDelete: () => void;
  onResized: (rect: Rect) => void;
};
export type GroupNodeType = Node<GroupNodeData, "canvasGroup">;

const SLOTS = Array.from({ length: 8 }, (_, i) => `slot-${i + 1}`);

const PHONE = "(max-width: 639px)";
function subscribePhone(onChange: () => void) {
  const m = window.matchMedia?.(PHONE);
  m?.addEventListener("change", onChange);
  return () => m?.removeEventListener("change", onChange);
}
const isPhone = () => window.matchMedia?.(PHONE).matches ?? false;

/**
 * A labelled area behind cards. Grab the label to move it with everything
 * inside; the inside lets clicks through, so the canvas still pans there.
 */
export function GroupNode({ id, data, selected }: NodeProps<GroupNodeType>) {
  const color = projectColor(data.color);
  const editing = useCanvasUi((s) => s.editingGroupId === id);
  const editGroup = useCanvasUi((s) => s.editGroup);
  const phone = useSyncExternalStore(subscribePhone, isPhone, () => false);
  const count = useStore(
    useCallback(
      (s: ReactFlowState) => {
        const me = s.nodeLookup.get(id);
        if (!me) return 0;
        const rect = { ...me.internals.positionAbsolute, width: me.measured.width ?? 0, height: me.measured.height ?? 0 };
        const cards = [];
        for (const n of s.nodeLookup.values()) {
          if (n.type !== "project" && n.type !== "locked") continue;
          cards.push({
            id: n.id,
            isGroup: false,
            ...n.internals.positionAbsolute,
            width: n.measured.width ?? CARD_WIDTH,
            height: n.measured.height ?? 140,
          });
        }
        return cardCount(rect, cards);
      },
      [id],
    ),
  );

  return (
    <>
      <NodeResizer
        isVisible={selected}
        minWidth={MIN_GROUP}
        minHeight={MIN_GROUP}
        color={color}
        handleClassName="tmp-group-resize"
        lineClassName="tmp-group-resize-line"
        onResizeEnd={(_, p) => data.onResized({ x: Math.round(p.x), y: Math.round(p.y), width: Math.round(p.width), height: Math.round(p.height) })}
      />
      {phone ? (
        // On a phone the group is often wider than the screen, so its tools
        // sit in a bar above the tabs instead of beside it.
        selected &&
        !editing &&
        createPortal(
          <div className="fixed inset-x-3 bottom-[calc(env(safe-area-inset-bottom)+84px)] z-30">
            <GroupToolbar data={data} onRename={() => editGroup(id)} stacked />
          </div>,
          document.body,
        )
      ) : (
        <NodeToolbar isVisible={selected && !editing} position={Position.Top} align="start" offset={10} style={{ zIndex: 2000 }}>
          <GroupToolbar data={data} onRename={() => editGroup(id)} />
        </NodeToolbar>
      )}
      <div
        className="tmp-group h-full w-full rounded-lg"
        style={{
          border: `var(--tmp-rule-width) solid ${color}`,
          background: `color-mix(in srgb, ${color} 7%, transparent)`,
          boxShadow: selected ? `4px 4px 0 0 ${color}` : undefined,
        }}
      >
        <div
          className="tmp-group-handle text-surface inline-flex max-w-full min-h-9 items-center gap-2.5 rounded-tl-[5px] rounded-br-md py-1 pr-3 pl-3"
          style={{ background: color }}
          onDoubleClick={(e) => {
            e.stopPropagation();
            editGroup(id);
          }}
          title="Drag to move the group and everything in it. Double-click to rename."
        >
          {editing ? (
            <LabelInput
              initial={data.label}
              onDone={(label) => {
                editGroup(null);
                if (label && label !== data.label) data.onRename(label);
              }}
            />
          ) : (
            <span className="font-display truncate text-[19px] leading-none font-extrabold tracking-[-0.02em]">{data.label}</span>
          )}
          <span className="label-mono shrink-0 text-[12px] opacity-80" aria-label={`${count} projects inside`}>
            {count}
          </span>
        </div>
      </div>
    </>
  );
}

function LabelInput({ initial, onDone }: { initial: string; onDone: (label: string | null) => void }) {
  const [value, setValue] = useState(initial);
  const ref = useRef<HTMLInputElement>(null);
  const done = useRef(false);
  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);
  const finish = (label: string | null) => {
    if (done.current) return;
    done.current = true;
    onDone(label);
  };
  return (
    <input
      ref={ref}
      aria-label="Group name"
      className="nodrag nopan font-display text-surface w-56 max-w-full bg-transparent text-[19px] leading-none font-extrabold tracking-[-0.02em] outline-none placeholder:opacity-60"
      value={value}
      maxLength={80}
      placeholder="Name this group"
      onChange={(e) => setValue(e.target.value)}
      onBlur={() => finish(value.trim() || null)}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") finish(value.trim() || null);
        if (e.key === "Escape") finish(null);
      }}
    />
  );
}

function GroupToolbar({ data, onRename, stacked = false }: { data: GroupNodeData; onRename: () => void; stacked?: boolean }) {
  const btn = "label-mono text-text hover:bg-surface-raised min-h-10 cursor-pointer rounded-md px-2.5";
  const rename = (
    <button type="button" className={btn} onClick={onRename}>Rename</button>
  );
  const remove = (
    <button type="button" className={`${btn} text-late`} onClick={data.onDelete} title="Remove the group. The projects stay where they are.">
      Remove
    </button>
  );
  const swatches = (
    <div className={`flex items-center ${stacked ? "justify-between" : ""}`} role="radiogroup" aria-label="Group colour">
      {SLOTS.map((slot, i) => {
        const on = data.color === slot;
        return (
          <button
            key={slot}
            type="button"
            role="radio"
            aria-checked={on}
            aria-label={`Colour ${i + 1}`}
            className={`grid cursor-pointer place-items-center rounded-md border-2 ${stacked ? "size-10" : "size-8"} ${on ? "border-text" : "border-transparent"}`}
            onClick={() => data.onRecolor(slot)}
          >
            <span className="size-4.5 rounded-sm" style={{ background: projectColor(slot) }} />
          </button>
        );
      })}
    </div>
  );
  const rule = <span className="bg-line mx-0.5 h-6 w-px" aria-hidden="true" />;
  if (stacked) {
    return (
      <div className="border-rule bg-surface-card shadow-card flex flex-col gap-1 rounded-lg p-1.5">
        <div className="flex items-center justify-between">
          <span className="label-mono text-text-quiet px-2.5">{data.label}</span>
          <span className="flex">{rename}{remove}</span>
        </div>
        {swatches}
      </div>
    );
  }
  return (
    <div className="border-rule bg-surface-card shadow-card flex items-center gap-1 rounded-lg p-1">
      {rename}
      {rule}
      {swatches}
      {rule}
      {remove}
    </div>
  );
}
