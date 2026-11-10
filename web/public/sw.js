// Service worker for the installed app. It does one thing: catch what a
// phone's share sheet posts to /share, keep it for the page, and open Quick
// add. Everything else goes to the network as usual (no offline caching yet).

const SHARE_CACHE = "tmp-share";

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));

self.addEventListener("fetch", (event) => {
  const url = new URL(event.request.url);
  if (event.request.method !== "POST" || url.origin !== self.location.origin || url.pathname !== "/share") return;
  event.respondWith(
    (async () => {
      try {
        const form = await event.request.formData();
        const cache = await caches.open(SHARE_CACHE);
        for (const key of await cache.keys()) await cache.delete(key);
        const files = form.getAll("files").filter((f) => typeof f !== "string" && f.size > 0);
        const meta = {
          title: form.get("title") || "",
          text: form.get("text") || "",
          url: form.get("url") || "",
          files: files.map((f, i) => ({ path: `/share/file/${i}`, name: f.name || `photo-${i + 1}.jpg`, type: f.type })),
          at: Date.now(),
        };
        await cache.put("/share/meta", new Response(JSON.stringify(meta), { headers: { "content-type": "application/json" } }));
        await Promise.all(files.map((f, i) => cache.put(`/share/file/${i}`, new Response(f, { headers: { "content-type": f.type || "application/octet-stream" } }))));
      } catch (e) {
        // Fall through to Quick add with nothing filled in.
      }
      return Response.redirect("/today?shared=1", 303);
    })(),
  );
});
