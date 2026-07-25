// Minimal offline static-asset cache, shared by every page (homepage + each game's own
// manifest-scoped app — see xtask::sw_register_bridge). Stale-while-revalidate: serve a
// cached response instantly if one exists, then refresh the cache from the network in the
// background; with nothing cached yet, wait on the network and fall back to cache (which
// will still be empty) only on failure. No precache list — assets fill in as they're
// visited, which is enough for "revisit while offline" without hand-maintaining a manifest
// of every game's .wasm/preview/font files.
const CACHE = "hcg-v1";

self.addEventListener("install", () => {
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

self.addEventListener("fetch", (event) => {
  if (event.request.method !== "GET") return;
  const url = new URL(event.request.url);
  if (url.origin !== location.origin) return;

  event.respondWith(
    caches.open(CACHE).then((cache) =>
      cache.match(event.request).then((cached) => {
        const network = fetch(event.request)
          .then((response) => {
            if (response.ok) cache.put(event.request, response.clone());
            return response;
          })
          .catch(() => cached);
        return cached || network;
      })
    )
  );
});
