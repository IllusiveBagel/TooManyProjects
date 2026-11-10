import { fromShare } from "./format";
import type { QuickAddStart } from "./QuickAdd";

const SHARE_CACHE = "tmp-share";

/** What the service worker kept from a phone's share sheet, then forgets it. */
export async function takeShared(): Promise<QuickAddStart | null> {
  if (!("caches" in window)) return null;
  try {
    const cache = await caches.open(SHARE_CACHE);
    const metaRes = await cache.match("/share/meta");
    if (!metaRes) return null;
    const meta = (await metaRes.json()) as {
      title: string;
      text: string;
      url: string;
      files: { path: string; name: string; type: string }[];
    };
    const files: File[] = [];
    for (const f of meta.files) {
      const res = await cache.match(f.path);
      if (res) files.push(new File([await res.blob()], f.name, { type: f.type }));
    }
    for (const key of await cache.keys()) await cache.delete(key);
    const { text, url, body } = fromShare(meta);
    return { text, url, body, files, source: "Shared from your phone", kind: files.length && !url && !text ? "inbox" : undefined };
  } catch {
    return null;
  }
}

/**
 * A share sent by link, as an iPhone Shortcut does:
 * `/today?add=1&text=…&url=…`. Returns null when the address carries none.
 */
export function sharedFromParams(params: URLSearchParams): QuickAddStart | null {
  if (!params.has("add")) return null;
  const { text, url, body } = fromShare({ title: params.get("title"), text: params.get("text"), url: params.get("url") });
  return { text, url, body, source: url || text ? "Shared" : undefined };
}
