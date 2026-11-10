import { autocompletion, closeBrackets, closeBracketsKeymap, completionKeymap, type CompletionContext } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap, placeholder as cmPlaceholder } from "@codemirror/view";
import { tags as t } from "@lezer/highlight";
import { useEffect, useRef } from "react";
import { openWikiQuery } from "./format";

export interface LinkTargets {
  notes: string[];
  projects: string[];
  /** Database views to embed, as `Database#View`. */
  views?: string[];
}

const theme = EditorView.theme({
  "&": { color: "var(--tmp-text)", backgroundColor: "transparent", fontSize: "16px" },
  "&.cm-focused": { outline: "none" },
  ".cm-content": { fontFamily: "var(--tmp-font-mono)", lineHeight: "1.7", padding: "8px 0", caretColor: "var(--tmp-accent)" },
  ".cm-line": { padding: "0" },
  ".cm-cursor": { borderLeftColor: "var(--tmp-accent)", borderLeftWidth: "2px" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection": {
    backgroundColor: "color-mix(in oklab, var(--tmp-accent) 30%, transparent) !important",
  },
  ".cm-placeholder": { color: "var(--tmp-text-quiet)" },
  ".cm-tooltip": {
    backgroundColor: "var(--tmp-surface-card)",
    border: "1.5px solid var(--tmp-line)",
    borderRadius: "var(--tmp-radius-md)",
    boxShadow: "var(--tmp-card-shadow)",
    overflow: "hidden",
  },
  ".cm-tooltip-autocomplete > ul": { fontFamily: "var(--tmp-font-interface)", maxHeight: "16em" },
  ".cm-tooltip-autocomplete > ul > li": { padding: "6px 10px !important" },
  ".cm-tooltip-autocomplete > ul > li[aria-selected]": {
    backgroundColor: "var(--tmp-accent)",
    color: "var(--tmp-on-accent)",
  },
  ".cm-completionDetail": { fontFamily: "var(--tmp-font-mono)", fontSize: "11px", marginLeft: "8px", opacity: "0.8" },
});

const highlight = HighlightStyle.define([
  { tag: t.heading1, fontWeight: "800", fontSize: "1.35em", fontFamily: "var(--tmp-font-display)" },
  { tag: t.heading2, fontWeight: "800", fontSize: "1.2em", fontFamily: "var(--tmp-font-display)" },
  { tag: [t.heading3, t.heading4, t.heading5, t.heading6], fontWeight: "800", fontFamily: "var(--tmp-font-display)" },
  { tag: t.strong, fontWeight: "700" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.strikethrough, textDecoration: "line-through" },
  { tag: [t.link, t.url], color: "var(--tmp-accent)" },
  { tag: [t.processingInstruction, t.meta, t.contentSeparator], color: "var(--tmp-text-quiet)" },
  { tag: t.monospace, color: "var(--tmp-slot-6)" },
  { tag: t.quote, color: "var(--tmp-text-quiet)", fontStyle: "italic" },
  { tag: t.list, color: "var(--tmp-accent)" },
]);

/** Suggests note titles after `[[` and project titles after `[[@`. */
function wikiCompletions(targets: () => LinkTargets) {
  return (ctx: CompletionContext) => {
    const line = ctx.state.doc.lineAt(ctx.pos);
    const open = openWikiQuery(line.text.slice(0, ctx.pos - line.from));
    if (!open) return null;
    const { notes, projects } = targets();
    const names = open.project ? projects : notes;
    return {
      from: line.from + open.from,
      options: names.map((name) => ({
        label: name,
        detail: open.project ? "project" : undefined,
        // Fill in the name, close the link if needed, and put the cursor after it.
        apply: (view: EditorView, _c: unknown, from: number, to: number) => {
          const closed = view.state.sliceDoc(to, to + 2) === "]]";
          const insert = closed ? name : `${name}]]`;
          view.dispatch({
            changes: { from, to, insert },
            selection: { anchor: from + name.length + 2 },
            userEvent: "input.complete",
          });
        },
      })),
      validFor: /^[^\]|#\n]*$/,
    };
  };
}

/** `/database` (or any start of it) at the start of a line offers views to embed. */
function slashCompletions(targets: () => LinkTargets) {
  return (ctx: CompletionContext) => {
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = line.text.slice(0, ctx.pos - line.from);
    const m = /^(\s*)\/([a-z ]*)$/i.exec(before);
    if (!m || !"database view".startsWith(m[2]!.toLowerCase())) return null;
    const views = targets().views ?? [];
    return {
      from: line.from + m[1]!.length,
      options: views.map((v) => ({
        label: `/database ${v.replace("#", " · ")}`,
        detail: "embed",
        apply: `![[${v}]]`,
      })),
      filter: false,
    };
  };
}

/**
 * Markdown source editor. It owns its text after mount: `value` is read once
 * (remount with a `key` to load another note) and changes go out through
 * `onChange`.
 */
export function MarkdownEditor({
  value,
  onChange,
  targets,
  placeholder = "Write in markdown. [[ links a note, [[@ links a project.",
  autoFocus,
  label,
  onFiles,
}: {
  /** Files pasted or dropped in: upload them and return the text to insert, such as `![[photo.jpg]]`. */
  onFiles?: (files: File[]) => Promise<string>;
  value: string;
  onChange: (text: string) => void;
  targets: LinkTargets;
  placeholder?: string;
  autoFocus?: boolean;
  label: string;
}) {
  const host = useRef<HTMLDivElement>(null);
  const changeRef = useRef(onChange);
  const targetsRef = useRef(targets);
  const filesRef = useRef(onFiles);
  changeRef.current = onChange;
  targetsRef.current = targets;
  filesRef.current = onFiles;

  useEffect(() => {
    const view = new EditorView({
      parent: host.current!,
      state: EditorState.create({
        doc: value,
        extensions: [
          history(),
          closeBrackets(),
          markdown({ base: markdownLanguage }),
          syntaxHighlighting(highlight),
          autocompletion({
            override: [wikiCompletions(() => targetsRef.current), slashCompletions(() => targetsRef.current)],
            icons: false,
          }),
          keymap.of([...closeBracketsKeymap, ...completionKeymap, ...defaultKeymap, ...historyKeymap, indentWithTab]),
          EditorView.lineWrapping,
          cmPlaceholder(placeholder),
          EditorView.contentAttributes.of({ "aria-label": label, "aria-multiline": "true" }),
          theme,
          EditorView.domEventHandlers({
            paste(e, view) {
              const files = Array.from(e.clipboardData?.files ?? []);
              if (!files.length || !filesRef.current) return false;
              e.preventDefault();
              const at = view.state.selection.main;
              void insertFiles(view, files, at.from, at.to);
              return true;
            },
            drop(e, view) {
              const files = Array.from(e.dataTransfer?.files ?? []);
              if (!files.length || !filesRef.current) return false;
              e.preventDefault();
              const pos = view.posAtCoords({ x: e.clientX, y: e.clientY }) ?? view.state.selection.main.head;
              void insertFiles(view, files, pos, pos);
              return true;
            },
          }),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) changeRef.current(u.state.doc.toString());
          }),
        ],
      }),
    });
    /** Holds the place with a marker while the files upload, then swaps in their embeds. */
    async function insertFiles(view: EditorView, files: File[], from: number, to: number) {
      const marker = `<!-- uploading ${files.length === 1 ? files[0]!.name : `${files.length} files`}… -->`;
      view.dispatch({ changes: { from, to, insert: marker }, selection: { anchor: from + marker.length } });
      let text = "";
      try {
        text = await filesRef.current!(files);
      } catch (err) {
        text = "";
        alert(err instanceof Error ? err.message : "Upload failed.");
      }
      const at = view.state.doc.toString().indexOf(marker);
      if (at >= 0) view.dispatch({ changes: { from: at, to: at + marker.length, insert: text }, selection: { anchor: at + text.length } });
    }
    if (autoFocus) {
      view.focus();
      view.dispatch({ selection: { anchor: view.state.doc.length } });
    }
    return () => view.destroy();
    // `value` is the starting text only; see the doc comment.
  }, []);

  return <div ref={host} className="min-h-[50vh]" data-testid="note-editor" />;
}
