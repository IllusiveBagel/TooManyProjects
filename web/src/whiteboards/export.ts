// Export a board as a self-contained SVG or a PNG, in the browser: theme
// colours written out, photos and the app's fonts embedded, so the file looks
// the same anywhere.
import { createElement } from "react";
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import { fileUrl } from "../files/format";
import { bounds, resolvedFonts, type El } from "./model";
import { Preview } from "./Scene";

const PAD = 40;

function asDataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result));
    r.onerror = () => reject(r.error);
    r.readAsDataURL(blob);
  });
}

async function fetchData(url: string): Promise<string | null> {
  try {
    const res = await fetch(url, { credentials: "same-origin" });
    return res.ok ? await asDataUrl(await res.blob()) : null;
  } catch {
    return null;
  }
}

/** The app's own @font-face rules (Latin only, to keep files small), with the fonts inlined. */
async function fontFaces(): Promise<string> {
  const out: string[] = [];
  for (const sheet of [...document.styleSheets]) {
    let rules: CSSRuleList;
    try {
      rules = sheet.cssRules;
    } catch {
      continue;
    }
    for (const rule of [...rules]) {
      if (!(rule instanceof CSSFontFaceRule)) continue;
      const range = rule.style.getPropertyValue("unicode-range");
      if (range && !/U\+0+-0*FF\b/i.test(range)) continue;
      const src = rule.style.getPropertyValue("src");
      if (/url\(["']?data:/.test(src)) {
        out.push(rule.cssText);
        continue;
      }
      const m = /url\(["']?([^"')]+\.woff2)["']?\)/.exec(src);
      if (!m) continue;
      const data = await fetchData(new URL(m[1]!, sheet.href ?? location.href).href);
      if (!data) continue;
      const css = rule.cssText.replace(/src:[^;]+;/, `src: url(${data}) format("woff2");`);
      out.push(css);
    }
  }
  return out.join("\n");
}

/** Writes theme variables out as their current values. */
function resolveVars(svg: string): string {
  const s = getComputedStyle(document.documentElement);
  // Nested vars resolve in a few passes.
  for (let i = 0; i < 3 && svg.includes("var(--"); i++) {
    svg = svg.replace(/var\((--[\w-]+)\)/g, (_, name: string) => s.getPropertyValue(name).trim() || "currentColor");
  }
  return svg;
}

/** The board as an SVG document. */
export async function boardSvg(els: El[], scale = 1): Promise<{ svg: string; width: number; height: number }> {
  const fonts = resolvedFonts();
  const b = bounds(els, fonts) ?? { x: 0, y: 0, w: 400, h: 240 };
  const width = Math.ceil((b.w + PAD * 2) * scale);
  const height = Math.ceil((b.h + PAD * 2) * scale);
  const photos = new Map<string, string>();
  for (const e of els) {
    if (e.kind === "image" && !photos.has(e.file)) {
      const data = await fetchData(fileUrl({ id: e.file }, 1600));
      if (data) photos.set(e.file, data);
    }
  }
  const host = document.createElement("div");
  const root = createRoot(host);
  flushSync(() => {
    root.render(createElement(Preview, { els, measure: fonts, pad: PAD, imageHref: (f: string) => photos.get(f) ?? "" }));
  });
  const node = host.querySelector("svg")!;
  node.setAttribute("xmlns", "http://www.w3.org/2000/svg");
  node.setAttribute("width", String(width));
  node.setAttribute("height", String(height));
  node.removeAttribute("class");
  node.removeAttribute("aria-hidden");
  const [vx, vy, vw, vh] = (node.getAttribute("viewBox") ?? "0 0 0 0").split(" ");
  const bg = document.createElementNS("http://www.w3.org/2000/svg", "rect");
  bg.setAttribute("x", vx!);
  bg.setAttribute("y", vy!);
  bg.setAttribute("width", vw!);
  bg.setAttribute("height", vh!);
  bg.setAttribute("fill", "var(--tmp-surface)");
  node.insertBefore(bg, node.firstChild);
  const style = document.createElementNS("http://www.w3.org/2000/svg", "style");
  style.textContent = await fontFaces();
  node.insertBefore(style, node.firstChild);
  const svg = resolveVars(new XMLSerializer().serializeToString(node));
  root.unmount();
  return { svg, width, height };
}

function save(blob: Blob, name: string) {
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(a.href), 10_000);
}

const fileName = (title: string, ext: string) =>
  `${title.replace(/[\\/:*?"<>|]+/g, "").trim() || "Whiteboard"}.${ext}`;

export async function downloadSvg(title: string, els: El[]) {
  const { svg } = await boardSvg(els);
  save(new Blob([svg], { type: "image/svg+xml" }), fileName(title, "svg"));
}

export async function downloadPng(title: string, els: El[]) {
  const { svg, width, height } = await boardSvg(els, 2);
  const img = new Image();
  img.src = URL.createObjectURL(new Blob([svg], { type: "image/svg+xml" }));
  await img.decode();
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  canvas.getContext("2d")!.drawImage(img, 0, 0, width, height);
  URL.revokeObjectURL(img.src);
  const blob = await new Promise<Blob | null>((r) => canvas.toBlob(r, "image/png"));
  if (blob) save(blob, fileName(title, "png"));
}
