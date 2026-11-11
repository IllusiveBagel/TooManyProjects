import { describe, expect, it } from "vitest";
import { ago, externalHost, folderTree, openWikiQuery, parseTags, parseWiki, readWikiHref, snippetParts, vaultFile, wikiToMarkdown } from "./format";

describe("parseWiki", () => {
  it("reads titles, aliases, headings and projects", () => {
    expect(parseWiki("Brakes")).toEqual({ project: false, title: "Brakes", heading: undefined, label: "Brakes" });
    expect(parseWiki("Brakes#Front|the brakes")).toMatchObject({ title: "Brakes", heading: "Front", label: "the brakes" });
    expect(parseWiki("@Car restoration")).toMatchObject({ project: true, title: "Car restoration", label: "@Car restoration" });
    expect(parseWiki("Knowledge/Cars/Brakes.md")).toMatchObject({ title: "Brakes" });
    expect(parseWiki("photo.png")).toBeNull();
    expect(parseWiki("  ")).toBeNull();
  });
});

describe("wikiToMarkdown", () => {
  it("turns wiki links into markdown links outside code", () => {
    const body = "See [[Brakes|the [brakes]]] and [[@Car]].\n`[[not]]`\n```\n[[fenced]]\n```\n![[photo.png]]";
    expect(wikiToMarkdown(body)).toBe(
      "See [the \\[brakes](wiki:Brakes)] and [@Car](project:Car).\n`[[not]]`\n```\n[[fenced]]\n```\n![photo.png](vault:photo.png)",
    );
  });

  it("round-trips titles through the href", () => {
    const md = wikiToMarkdown("[[Paint & primer?]]");
    const href = /\((.*)\)/.exec(md)![1]!;
    expect(readWikiHref(href)).toEqual({ project: false, title: "Paint & primer?" });
    expect(readWikiHref("https://example.com")).toBeNull();
  });
});

describe("openWikiQuery", () => {
  it("finds an unfinished link before the cursor", () => {
    expect(openWikiQuery("see [[Bra")).toEqual({ from: 6, query: "Bra", project: false });
    expect(openWikiQuery("see [[@Ca")).toEqual({ from: 7, query: "Ca", project: true });
    expect(openWikiQuery("see [[Brakes]] and")).toBeNull();
    expect(openWikiQuery("no link")).toBeNull();
  });
});

describe("folderTree", () => {
  it("builds folders with counts, the fixed three first", () => {
    const tree = folderTree([
      "Knowledge/Cars/Brakes.md",
      "Knowledge/Cars/Paint.md",
      "Archive/Old.md",
      "Inbox/2026-11-06.md",
      "Projects/Car/Plan.md",
      "Loose.md",
    ]);
    expect(tree.map((n) => [n.name, n.count])).toEqual([
      ["Inbox", 1],
      ["Knowledge", 2],
      ["Projects", 1],
      ["Archive", 1],
    ]);
    expect(tree[1]!.children[0]).toMatchObject({ name: "Cars", path: "Knowledge/Cars", count: 2 });
  });
});

it("splits search snippets into hits", () => {
  expect(snippetParts("…pump the [pedal] slowly")).toEqual([
    { text: "…pump the ", hit: false },
    { text: "pedal", hit: true },
    { text: " slowly", hit: false },
  ]);
});

it("parses typed tags", () => {
  expect(parseTags("car, how to #brakes, Car")).toEqual(["car", "how-to", "brakes"]);
});

it("says how long ago", () => {
  const now = Date.parse("2026-11-06T12:00:00Z");
  expect(ago("2026-11-06T11:59:50Z", now)).toBe("just now");
  expect(ago("2026-11-06T11:55:00Z", now)).toBe("5 min ago");
  expect(ago("2026-11-05T12:00:00Z", now)).toBe("yesterday");
});

describe("database embeds", () => {
  it("turns a whole-line embed into a block the renderer shows as a view", () => {
    expect(wikiToMarkdown("Parts:\n![[Parts list#To order]]\nDone")).toBe("Parts:\n\n```tmp-embed\nParts list#To order\n```\n\nDone");
    // Files and inline embeds are left alone.
    expect(wikiToMarkdown("![[photo.jpg]]")).toBe("![photo.jpg](vault:photo.jpg)");
    expect(wikiToMarkdown("![[my photo.jpg|300]]")).toBe("![my photo.jpg](vault:my%20photo.jpg)");
    expect(vaultFile("vault:my%20photo.jpg")).toBe("my photo.jpg");
    expect(vaultFile("Car/photo.png")).toBe("Car/photo.png");
    expect(vaultFile("https://x.com/a.png")).toBeNull();
    expect(wikiToMarkdown("see ![[Parts list]] here")).toBe("see Parts list here");
  });
});

describe("externalHost", () => {
  it("names other sites only", () => {
    const here = "https://projects.example.com";
    expect(externalHost("https://tracker.evil.test/p.gif", here)).toBe("tracker.evil.test");
    expect(externalHost("//cdn.test/a.png", here)).toBe("cdn.test");
    expect(externalHost("https://projects.example.com/api/v1/x.png", here)).toBeNull();
    expect(externalHost("vault:photo.png", here)).toBeNull();
    expect(externalHost("photo.png", here)).toBeNull();
  });
});
