import { syntaxTree } from "@codemirror/language";
import { RangeSetBuilder } from "@codemirror/state";
import { Decoration, type DecorationSet, type EditorView, ViewPlugin, type ViewUpdate } from "@codemirror/view";

/** Markdown nodes that aren't prose: the browser shouldn't underline words in them. */
const NOT_PROSE = new Set(["InlineCode", "FencedCode", "CodeBlock", "URL", "Autolink", "HTMLBlock", "HTMLTag", "Comment", "CommentBlock"]);

const quiet = Decoration.mark({ attributes: { spellcheck: "false" } });

/** `[[Note]]`, `[[@Project]]`, `![[photo.jpg]]` and `#tags`: names, not words. */
const NAMES = /!?\[\[[^\]\n]*\]\]|(?<![\w/])#[\p{L}\p{N}_/-]+/gu;

/** Ranges in the visible part of the note that spell check should skip, in order. */
export function quietRanges(view: Pick<EditorView, "state" | "visibleRanges">): [number, number][] {
  const out: [number, number][] = [];
  const doc = view.state.doc;
  // Front matter at the top of a note (`---` ... `---`) is data, not prose.
  if (doc.lines > 1 && doc.line(1).text === "---") {
    for (let i = 2; i <= doc.lines; i++) {
      if (doc.line(i).text === "---") {
        out.push([0, doc.line(i).to]);
        break;
      }
    }
  }
  for (const { from, to } of view.visibleRanges) {
    syntaxTree(view.state).iterate({
      from,
      to,
      enter: (n) => {
        if (!NOT_PROSE.has(n.name)) return;
        out.push([n.from, n.to]);
        return false;
      },
    });
    const text = doc.sliceString(from, to);
    for (const m of text.matchAll(NAMES)) out.push([from + m.index, from + m.index + m[0].length]);
  }
  out.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  // Merge overlaps so the builder gets ranges in order.
  const merged: [number, number][] = [];
  for (const r of out) {
    const last = merged[merged.length - 1];
    if (last && r[0] <= last[1]) last[1] = Math.max(last[1], r[1]);
    else if (r[1] > r[0]) merged.push([...r]);
  }
  return merged;
}

function build(view: EditorView): DecorationSet {
  const b = new RangeSetBuilder<Decoration>();
  for (const [from, to] of quietRanges(view)) b.add(from, to, quiet);
  return b.finish();
}

/**
 * Turns the browser's spell check off inside code, links, tags and front
 * matter, so only the writing gets red underlines.
 */
export const proseOnlySpellcheck = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = build(view);
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.viewportChanged || syntaxTree(u.startState) !== syntaxTree(u.state)) this.decorations = build(u.view);
    }
  },
  { decorations: (v) => v.decorations },
);
