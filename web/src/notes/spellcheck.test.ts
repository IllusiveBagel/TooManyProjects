import { ensureSyntaxTree } from "@codemirror/language";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { EditorState } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import { quietRanges } from "./spellcheck";

function quiet(doc: string): string[] {
  const state = EditorState.create({ doc, extensions: [markdown({ base: markdownLanguage })] });
  ensureSyntaxTree(state, doc.length, 5000);
  return quietRanges({ state, visibleRanges: [{ from: 0, to: doc.length }] }).map(([a, b]) => doc.slice(a, b));
}

describe("quietRanges", () => {
  it("leaves plain prose alone", () => {
    expect(quiet("Teh quick brown fox")).toEqual([]);
  });

  it("skips code, links, tags and wiki links", () => {
    expect(quiet("Run `cargo fmt` on [[Brake notes]] for [[@Beetle]] #wip https://example.com")).toEqual([
      "`cargo fmt`",
      "[[Brake notes]]",
      "[[@Beetle]]",
      "#wip",
      "https://example.com",
    ]);
  });

  it("skips fenced code and front matter", () => {
    const doc = "---\nid: abc\n---\nSome text\n\n```rust\nfn mian() {}\n```\n";
    expect(quiet(doc)).toEqual(["---\nid: abc\n---", "```rust\nfn mian() {}\n```"]);
  });

  it("does not treat a heading as a tag", () => {
    expect(quiet("# Heading\nA#b")).toEqual([]);
  });
});
