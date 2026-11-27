// Draws whiteboard elements as SVG in the app's own style. `Scene` draws the
// board itself (in board units) for the editor to place; `Preview` fits a
// whole board into a box, for the list and for export.
import { getStroke } from "perfect-freehand";
import { memo, useId, useMemo, type ReactNode } from "react";
import { fileUrl } from "../files/format";
import {
  arrowEnds,
  bounds,
  CSS_FONTS,
  LINE,
  measure as measureText,
  SLOTS,
  TEXT_SIZE,
  wrap,
  type ArrowEl,
  type El,
  type Fonts,
  type ImageEl,
  type Pt,
  type Rect,
  type ShapeEl,
  type Slot,
  type StickyEl,
  type TextEl,
} from "./model";

export const colorOf = (c: Slot) => (c === "ink" ? "var(--tmp-text)" : `var(--tmp-${c})`);
const tint = (c: Slot, pct: number) => `color-mix(in srgb, ${colorOf(c)} ${pct}%, var(--tmp-surface-card))`;
/** Sticky notes are paper in both themes, like the real thing. */
export const stickyFill = (c: Slot) => `color-mix(in srgb, ${colorOf(c)} 55%, #F2EFE8)`;
export const STICKY_INK = "#1C1A17";
export const SHAPE_TEXT = 17;
export const STICKY_TEXT = 16;
export const STICKY_PAD = 14;

export function strokePath(points: Pt[], size: number, last = true) {
  const s = getStroke(points, { size: size * 2.2, thinning: 0.5, smoothing: 0.6, streamline: 0.4, last });
  if (!s.length) return "";
  return s.reduce((d, p, i) => d + `${i ? "L" : "M"}${p[0]!.toFixed(1)},${p[1]!.toFixed(1)}`, "") + "Z";
}

/** What the scene needs besides the elements. */
export interface SceneProps {
  els: El[];
  /** Fonts to measure with (real family names). */
  measure: Fonts;
  /** Elements not to draw, such as the one whose words are being typed. */
  hide?: Set<string>;
  /** Where a photo's pixels come from; export swaps in data URLs. */
  imageHref?: (file: string) => string;
}

/** Markers and patterns the scene refers to. Put once inside the `<svg>`. */
export function SceneDefs({ pid }: { pid: string }) {
  return (
    <defs>
      <pattern id={`${pid}minor`} width="16" height="16" patternUnits="userSpaceOnUse">
        <path d="M16 0H0V16" fill="none" stroke="var(--tmp-canvas-grid-minor)" strokeWidth="1" />
      </pattern>
      <pattern id={`${pid}major`} width="128" height="128" patternUnits="userSpaceOnUse">
        <rect width="128" height="128" fill={`url(#${pid}minor)`} />
        <path d="M128 0H0V128" fill="none" stroke="var(--tmp-canvas-grid-major)" strokeWidth="1" />
      </pattern>
      {[...SLOTS, "accent" as const].map((c) => (
        <marker
          key={c}
          id={`${pid}head-${c}`}
          viewBox="0 0 10 10"
          refX="9"
          refY="5"
          markerWidth="9"
          markerHeight="9"
          orient="auto-start-reverse"
          markerUnits="userSpaceOnUse"
        >
          <path d="M0,0 L10,5 L0,10 Z" fill={c === "accent" ? "var(--tmp-accent)" : colorOf(c)} />
        </marker>
      ))}
    </defs>
  );
}

/** The board's elements, arrows first so they run under the shapes they join. */
export const Scene = memo(function Scene({ els, measure, hide, imageHref, pid }: SceneProps & { pid: string }) {
  const all = useMemo(() => new Map(els.map((e) => [e.id, e])), [els]);
  return (
    <g>
      {els.map((e) =>
        e.kind === "arrow" && !hide?.has(e.id) ? <Arrow key={e.id} e={e} all={all} fonts={measure} pid={pid} /> : null,
      )}
      {els.map((e) => {
        if (hide?.has(e.id)) return null;
        switch (e.kind) {
          case "box":
          case "ellipse":
          case "diamond":
            return <Shape key={e.id} e={e} fonts={measure} />;
          case "sticky":
            return <Sticky key={e.id} e={e} fonts={measure} />;
          case "text":
            return <Text key={e.id} e={e} />;
          case "pen":
            return <path key={e.id} d={strokePath(e.points, e.width)} fill={colorOf(e.color)} />;
          case "image":
            return <Photo key={e.id} e={e} href={imageHref} />;
          default:
            return null;
        }
      })}
    </g>
  );
});

export function Arrow({ e, all, fonts, pid }: { e: ArrowEl; all: Map<string, El>; fonts: Fonts; pid: string }) {
  const [[x1, y1], [x2, y2]] = arrowEnds(e, all, fonts);
  return (
    <g>
      <line
        x1={x1}
        y1={y1}
        x2={x2}
        y2={y2}
        stroke={colorOf(e.color)}
        strokeWidth={2.5}
        strokeLinecap="round"
        strokeDasharray={e.dashed ? "8 6" : undefined}
        markerEnd={e.head ? `url(#${pid}head-${e.color})` : undefined}
      />
      {e.label && <LinkLabel x={(x1 + x2) / 2} y={(y1 + y2) / 2} color={e.color} text={e.label} fonts={fonts} />}
    </g>
  );
}

/** How wide an arrow's label chip is: mono capitals with a little spacing. */
export function labelWidth(text: string, fonts: Fonts) {
  const t = text.toUpperCase();
  return measureText(t, fonts.label, 12) + t.length * 12 * 0.06 + 18;
}

function LinkLabel({ x, y, color, text, fonts }: { x: number; y: number; color: Slot; text: string; fonts: Fonts }) {
  const w = labelWidth(text, fonts);
  return (
    <g>
      <rect x={x - w / 2} y={y - 12} width={w} height={24} rx={3} fill="var(--tmp-surface)" stroke={colorOf(color)} strokeWidth={1.5} />
      <text
        x={x}
        y={y + 4.5}
        textAnchor="middle"
        fill={colorOf(color)}
        style={{ font: CSS_FONTS.label, letterSpacing: "0.06em", textTransform: "uppercase" }}
      >
        {text}
      </text>
    </g>
  );
}

function Lines({
  x,
  y,
  lines,
  size,
  anchor = "middle",
  font,
  fill,
}: {
  x: number;
  /** Middle of the block when centred, else its top. */
  y: number;
  lines: string[];
  size: number;
  anchor?: "start" | "middle";
  font: string;
  fill: string;
}) {
  const lh = size * LINE;
  const top = anchor === "middle" ? y - (lines.length * lh) / 2 : y;
  return (
    <text textAnchor={anchor} fill={fill} style={{ font, letterSpacing: "-0.01em" }}>
      {lines.map((l, i) => (
        <tspan key={i} x={x} y={top + i * lh + lh / 2 + size * 0.35}>
          {l || " "}
        </tspan>
      ))}
    </text>
  );
}

function Shape({ e, fonts }: { e: ShapeEl; fonts: Fonts }) {
  const stroke = colorOf(e.color);
  const fill =
    e.fill === "none" ? "transparent" : e.fill === "tint" ? tint(e.color, e.color === "ink" ? 6 : 16) : stroke;
  const textFill = e.fill === "solid" ? (e.color === "ink" ? "var(--tmp-surface)" : "var(--tmp-on-accent)") : "var(--tmp-text)";
  let body: ReactNode;
  if (e.kind === "ellipse")
    body = <ellipse cx={e.x + e.w / 2} cy={e.y + e.h / 2} rx={e.w / 2} ry={e.h / 2} fill={fill} stroke={stroke} strokeWidth={2} />;
  else if (e.kind === "diamond")
    body = (
      <path
        d={`M${e.x + e.w / 2},${e.y}L${e.x + e.w},${e.y + e.h / 2}L${e.x + e.w / 2},${e.y + e.h}L${e.x},${e.y + e.h / 2}Z`}
        fill={fill}
        stroke={stroke}
        strokeWidth={2}
        strokeLinejoin="round"
      />
    );
  else
    body = (
      <>
        {e.fill !== "none" && <rect x={e.x + 4} y={e.y + 4} width={e.w} height={e.h} rx={6} fill="var(--tmp-shadow)" />}
        <rect x={e.x} y={e.y} width={e.w} height={e.h} rx={6} fill={fill} stroke={stroke} strokeWidth={2} />
      </>
    );
  // Words sit inside the shape; a diamond or ellipse has less room.
  const room = e.kind === "box" ? e.w - 16 : e.w * 0.7;
  return (
    <g>
      {body}
      {e.text && (
        <Lines
          x={e.x + e.w / 2}
          y={e.y + e.h / 2}
          lines={wrap(e.text, Math.max(room, 20), fonts.shape, SHAPE_TEXT)}
          size={SHAPE_TEXT}
          font={CSS_FONTS.shape}
          fill={textFill}
        />
      )}
      {e.task && (
        <g>
          <rect x={e.x + e.w - 50} y={e.y - 11} width={48} height={20} rx={3} fill="var(--tmp-accent)" />
          <text
            x={e.x + e.w - 26}
            y={e.y + 3.5}
            textAnchor="middle"
            fill="var(--tmp-on-accent)"
            style={{ font: "700 11px var(--tmp-font-mono)", letterSpacing: "0.04em" }}
          >
            {e.task}
          </text>
        </g>
      )}
    </g>
  );
}

function Sticky({ e, fonts }: { e: StickyEl; fonts: Fonts }) {
  return (
    <g>
      <rect x={e.x + 5} y={e.y + 5} width={e.w} height={e.h} rx={3} fill="var(--tmp-shadow)" />
      <rect x={e.x} y={e.y} width={e.w} height={e.h} rx={3} fill={stickyFill(e.color)} />
      {e.text && (
        <Lines
          x={e.x + STICKY_PAD}
          y={e.y + 10}
          lines={wrap(e.text, e.w - STICKY_PAD * 2, fonts.sticky, STICKY_TEXT)}
          size={STICKY_TEXT}
          anchor="start"
          font={CSS_FONTS.sticky}
          fill={STICKY_INK}
        />
      )}
    </g>
  );
}

function Text({ e }: { e: TextEl }) {
  const size = TEXT_SIZE[e.size];
  return (
    <Lines x={e.x} y={e.y} lines={e.text.split("\n")} size={size} anchor="start" font={CSS_FONTS.text(e)} fill={colorOf(e.color)} />
  );
}

function Photo({ e, href }: { e: ImageEl; href?: (file: string) => string }) {
  return (
    <g>
      <rect x={e.x + 4} y={e.y + 4} width={e.w} height={e.h} rx={3} fill="var(--tmp-shadow)" />
      <rect x={e.x} y={e.y} width={e.w} height={e.h} rx={3} fill="var(--tmp-surface-raised)" />
      <image
        href={href ? href(e.file) : fileUrl({ id: e.file }, 1600)}
        x={e.x}
        y={e.y}
        width={e.w}
        height={e.h}
        preserveAspectRatio="xMidYMid slice"
      />
      <rect x={e.x} y={e.y} width={e.w} height={e.h} rx={3} fill="none" stroke="var(--tmp-line)" strokeWidth={1} />
    </g>
  );
}

/** Graph paper under the board, covering `r`. */
export function Grid({ pid, r }: { pid: string; r: Rect }) {
  return <rect x={r.x} y={r.y} width={r.w} height={r.h} fill={`url(#${pid}major)`} />;
}

/** A whole board fitted into its box. */
export function Preview({
  els,
  measure,
  grid = false,
  pad = 40,
  className = "",
  imageHref,
}: {
  els: El[];
  measure: Fonts;
  grid?: boolean;
  pad?: number;
  className?: string;
  imageHref?: (file: string) => string;
}) {
  const pid = useId().replace(/:/g, "");
  const b = bounds(els, measure) ?? { x: 0, y: 0, w: 400, h: 240 };
  const v = { x: b.x - pad, y: b.y - pad, w: b.w + pad * 2, h: b.h + pad * 2 };
  return (
    <svg className={`block h-full w-full ${className}`} viewBox={`${v.x} ${v.y} ${v.w} ${v.h}`} preserveAspectRatio="xMidYMid meet" aria-hidden="true">
      <SceneDefs pid={pid} />
      {grid && <Grid pid={pid} r={v} />}
      <Scene els={els} measure={measure} pid={pid} imageHref={imageHref} />
    </svg>
  );
}
